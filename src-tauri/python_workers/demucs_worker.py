#!/usr/bin/env python3
import contextlib
import importlib.util
import json
import os
import shutil
import signal
import sys
import traceback
from datetime import datetime
from pathlib import Path


def utc_ts():
    return datetime.utcnow().isoformat(timespec="milliseconds") + "Z"


class TeeStream:
    def __init__(self, original, *paths):
        self.original = original
        self.paths = [Path(p) for p in paths if p]

    def write(self, text):
        if not isinstance(text, str):
            text = str(text)
        try:
            self.original.write(text)
            self.original.flush()
        except Exception:
            pass
        for path in self.paths:
            try:
                path.parent.mkdir(parents=True, exist_ok=True)
                with path.open("a", encoding="utf-8", errors="replace") as handle:
                    handle.write(text)
            except Exception:
                pass
        return len(text)

    def flush(self):
        try:
            self.original.flush()
        except Exception:
            pass


class Worker:
    def __init__(self, job_file):
        self.job_file = Path(job_file)
        self.job = self.read_job()
        self.song_id = str(self.job.get("song_id", ""))
        self.job_token = str(self.job.get("job_token", ""))
        self.input_path = str(self.job["input_path"])
        self.output_dir = Path(self.job["output_dir"])
        self.device = str(self.job.get("device") or self.job.get("selected_device") or "cpu")
        self.prefer_demucs_cuda = bool(self.job.get("prefer_demucs_cuda", False))
        self.has_nvidia_gpu = bool(self.job.get("has_nvidia_gpu", False))

        self.debug_dir = self.output_dir / "debug"
        self.root_result_file = self.output_dir / "separator_result.json"
        self.root_progress_file = self.output_dir / "separator_progress.json"
        self.debug_result_file = self.debug_dir / "separator_result.json"
        self.debug_progress_file = self.debug_dir / "separator_progress.json"
        self.debug_log_path = self.debug_dir / "separator_debug.log"
        # Compatibility name: in fixed-worker mode this captures demucs_main stdout/stderr,
        # not output from a separate child process.
        self.demucs_log_path = self.debug_dir / "demucs_child.log"
        self.stdout_log_path = self.debug_dir / "stdout.log"
        self.stderr_log_path = self.debug_dir / "stderr.log"
        self.traceback_log_path = self.debug_dir / "traceback.log"
        self.command_file_path = self.debug_dir / "command.json"
        self.runtime_probe_path = self.debug_dir / "runtime_probe.json"
        self.debug_job_path = self.debug_dir / "job.json"
        self.demucs_cmd = []
        self.demucs_returncode = None
        self.runtime_probe = {}
        self.cancelled = False

    def read_job(self):
        with self.job_file.open("r", encoding="utf-8-sig") as handle:
            return json.load(handle)

    def append_debug(self, message):
        try:
            self.debug_log_path.parent.mkdir(parents=True, exist_ok=True)
            with self.debug_log_path.open("a", encoding="utf-8", errors="replace") as handle:
                handle.write(f"[{utc_ts()}] {message}\n")
        except Exception:
            pass

    def append_demucs(self, message):
        try:
            self.demucs_log_path.parent.mkdir(parents=True, exist_ok=True)
            with self.demucs_log_path.open("a", encoding="utf-8", errors="replace") as handle:
                handle.write(f"[{utc_ts()}] {message}\n")
        except Exception:
            pass

    def write_json(self, path, payload):
        path = Path(path)
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("w", encoding="utf-8") as handle:
            json.dump(payload, handle, ensure_ascii=False, indent=2)

    def write_progress(self, percent, message, stage):
        payload = {
            "percent": percent,
            "message": message,
            "stage": stage,
        }
        self.write_json(self.root_progress_file, payload)
        self.write_json(self.debug_progress_file, payload)

    def write_traceback(self, exc_text):
        try:
            self.traceback_log_path.parent.mkdir(parents=True, exist_ok=True)
            with self.traceback_log_path.open("a", encoding="utf-8", errors="replace") as handle:
                handle.write(f"[{utc_ts()}]\n{exc_text}\n")
        except Exception:
            pass
        self.append_debug("traceback follows")
        self.append_debug(exc_text)

    def prepare_debug_dir(self):
        self.output_dir.mkdir(parents=True, exist_ok=True)
        if self.debug_dir.exists():
            shutil.rmtree(self.debug_dir, ignore_errors=True)
        self.debug_dir.mkdir(parents=True, exist_ok=True)
        self.write_json(self.debug_job_path, self.job)
        for path in [
            self.stdout_log_path,
            self.stderr_log_path,
            self.traceback_log_path,
            self.demucs_log_path,
        ]:
            path.touch(exist_ok=True)
        self.append_debug("demucs_worker bootstrap")
        self.append_debug(f"song_id={self.song_id}")
        self.append_debug(f"job_token={self.job_token}")
        self.append_debug(f"input_path={self.input_path}")
        self.append_debug(f"output_dir={self.output_dir}")
        self.append_debug(f"selected_device={self.device}")
        self.append_debug(f"prefer_demucs_cuda={self.prefer_demucs_cuda}")
        self.append_debug(f"has_nvidia_gpu={self.has_nvidia_gpu}")
        self.append_debug(f"job_file={self.job_file}")
        self.append_debug(f"worker_path={Path(__file__).resolve()}")
        self.append_debug(f"sys.executable={sys.executable}")
        self.append_debug(f"sys.version={sys.version}")
        self.append_debug(f"cwd={os.getcwd()}")
        for key in [
            "PATH",
            "CUDA_PATH",
            "VIRTUAL_ENV",
            "CONDA_PREFIX",
            "PYTHONPATH",
            "TORCH_FORCE_WEIGHTS_ONLY_LOAD",
            "PYTHONUTF8",
            "PYTHONIOENCODING",
        ]:
            self.append_debug(f"{key}={os.environ.get(key, '')}")
        self.append_demucs(
            "demucs_child.log compatibility note: fixed worker captures demucs_main stdout/stderr in-process."
        )

    def patch_torchaudio_load(self):
        def _patched_load(uri, *args, **kwargs):
            import soundfile as sf
            import torch

            data, samplerate = sf.read(str(uri), dtype="float32")
            data = torch.from_numpy(data)
            if data.ndim == 1:
                data = data.unsqueeze(0)
            else:
                data = data.T
            return data, samplerate

        try:
            import torchaudio

            torchaudio.load = _patched_load
            self.append_debug("torchaudio.load patched inside fixed demucs worker process")
        except Exception:
            exc_text = traceback.format_exc()
            self.write_traceback(exc_text)
            raise

    def run_runtime_probe(self):
        probe = {
            "sys_executable": sys.executable,
            "sys_version": sys.version,
            "cwd": os.getcwd(),
            "worker_path": str(Path(__file__).resolve()),
        }
        errors = []
        for mod_name in ["torch", "torchaudio", "demucs", "demucs.separate", "soundfile", "numpy"]:
            try:
                spec = importlib.util.find_spec(mod_name)
                probe[f"{mod_name}_spec"] = getattr(spec, "origin", None) if spec else None
            except Exception:
                probe[f"{mod_name}_spec_error"] = traceback.format_exc()

        try:
            import torch

            probe["torch_version"] = getattr(torch, "__version__", None)
            probe["torch_file"] = getattr(torch, "__file__", None)
            probe["torch_cuda_version"] = getattr(torch.version, "cuda", None)
            probe["torch_cuda_available"] = bool(torch.cuda.is_available())
            probe["torch_cuda_device_name"] = (
                torch.cuda.get_device_name(0) if torch.cuda.is_available() else None
            )
        except Exception:
            errors.append(traceback.format_exc())

        try:
            import torchaudio

            probe["torchaudio_version"] = getattr(torchaudio, "__version__", None)
            probe["torchaudio_file"] = getattr(torchaudio, "__file__", None)
        except Exception:
            errors.append(traceback.format_exc())

        try:
            import demucs

            probe["demucs_file"] = getattr(demucs, "__file__", None)
        except Exception:
            errors.append(traceback.format_exc())

        try:
            import soundfile

            probe["soundfile_file"] = getattr(soundfile, "__file__", None)
            probe["soundfile_version"] = getattr(soundfile, "__version__", None)
        except Exception:
            errors.append(traceback.format_exc())

        try:
            import numpy

            probe["numpy_file"] = getattr(numpy, "__file__", None)
            probe["numpy_version"] = getattr(numpy, "__version__", None)
        except Exception:
            errors.append(traceback.format_exc())

        probe["errors"] = errors
        self.runtime_probe = probe
        self.write_json(self.runtime_probe_path, probe)
        self.append_debug("runtime probe written")
        if errors:
            for error in errors:
                self.write_traceback(error)

    def build_demucs_command(self):
        self.demucs_cmd = [
            "demucs",
            "--two-stems=vocals",
            "-n",
            "htdemucs_ft",
            "-o",
            str(self.output_dir),
            "--device",
            self.device,
            self.input_path,
        ]
        command_payload = {
            "demucs_cmd": self.demucs_cmd,
            "shell": False,
            "cwd": os.getcwd(),
            "python_executable": sys.executable,
            "worker_path": str(Path(__file__).resolve()),
            "env": {
                "PATH": os.environ.get("PATH", ""),
                "CUDA_PATH": os.environ.get("CUDA_PATH", ""),
                "TORCH_FORCE_WEIGHTS_ONLY_LOAD": os.environ.get(
                    "TORCH_FORCE_WEIGHTS_ONLY_LOAD", ""
                ),
                "PYTHONUTF8": os.environ.get("PYTHONUTF8", ""),
                "PYTHONIOENCODING": os.environ.get("PYTHONIOENCODING", ""),
            },
        }
        self.write_json(self.command_file_path, command_payload)
        self.append_debug(f"demucs argv={self.demucs_cmd!r}")

    def result_payload(self, success, error=None, error_code=None, stage=None, extra=None):
        probe = self.runtime_probe or {}
        payload = {
            "success": bool(success),
            "selected_device": self.device,
            "gpu_requested": bool(self.prefer_demucs_cuda),
            "has_nvidia_gpu": bool(self.has_nvidia_gpu),
            "torch_cuda_available": probe.get("torch_cuda_available"),
            "torch_version": probe.get("torch_version"),
            "torch_cuda_version": probe.get("torch_cuda_version"),
            "torch_cuda_device_name": probe.get("torch_cuda_device_name"),
            "demucs_device_arg": self.device,
            "debug_log_path": str(self.debug_log_path),
            "demucs_log_path": str(self.demucs_log_path),
            "job_file_path": str(self.debug_job_path),
            "worker_path": str(Path(__file__).resolve()),
            "command_file_path": str(self.command_file_path),
            "runtime_probe_path": str(self.runtime_probe_path),
            "demucs_returncode": self.demucs_returncode,
            "demucs_cmd": self.demucs_cmd,
            "python_executable": sys.executable,
            "demucs_file": probe.get("demucs_file"),
            "torchaudio_file": probe.get("torchaudio_file"),
            "torch_file": probe.get("torch_file"),
        }
        if error is not None:
            payload["error"] = str(error)
        if error_code is not None:
            payload["error_code"] = str(error_code)
        if stage is not None:
            payload["stage"] = str(stage)
        if extra:
            payload.update(extra)
        return payload

    def write_result(self, payload):
        self.write_json(self.root_result_file, payload)
        self.write_json(self.debug_result_file, payload)
        print(json.dumps(payload, ensure_ascii=False))

    def cleanup_debug_on_success(self):
        if os.environ.get("MACARON_KEEP_DEBUG") == "1":
            self.append_debug("MACARON_KEEP_DEBUG=1; preserving debug directory after success")
            return
        shutil.rmtree(self.debug_dir, ignore_errors=True)

    def handle_cancel_signal(self, signum, frame):
        self.cancelled = True
        self.demucs_returncode = 130
        payload = self.result_payload(
            False,
            error="Demucs 分离已取消",
            error_code="CANCELLED",
            stage="cancelled",
        )
        try:
            self.write_result(payload)
        finally:
            raise SystemExit(130)

    def install_signal_handlers(self):
        try:
            signal.signal(signal.SIGTERM, self.handle_cancel_signal)
            signal.signal(signal.SIGINT, self.handle_cancel_signal)
        except Exception:
            pass

    def run_demucs(self):
        self.write_progress(20, "人声分离中：检查运行环境", "checking")
        self.run_runtime_probe()
        self.write_progress(25, "人声分离中：准备 Demucs", "probe")
        self.build_demucs_command()
        self.patch_torchaudio_load()

        from demucs.separate import main as demucs_main

        original_argv = list(sys.argv)
        sys.argv = list(self.demucs_cmd)
        self.write_progress(30, "人声分离中：开始分离", "separating_started")
        self.append_demucs(f"demucs argv={sys.argv!r}")
        stdout_tee = TeeStream(sys.stdout, self.stdout_log_path, self.demucs_log_path)
        stderr_tee = TeeStream(sys.stderr, self.stderr_log_path, self.demucs_log_path)
        try:
            with contextlib.redirect_stdout(stdout_tee), contextlib.redirect_stderr(stderr_tee):
                result = demucs_main()
            if isinstance(result, int) and result != 0:
                self.demucs_returncode = result
                raise SystemExit(result)
            self.demucs_returncode = 0
        except SystemExit as exc:
            code = exc.code
            if code is None:
                code = 0
            if isinstance(code, str):
                self.demucs_returncode = 1
                raise RuntimeError(code)
            self.demucs_returncode = int(code)
            if self.demucs_returncode == 0:
                return
            raise
        finally:
            sys.argv = original_argv

    def validate_outputs(self):
        self.write_progress(90, "人声分离中：校验输出", "output_validation")
        filename = Path(self.input_path).stem or "Unknown"
        out_subdir = self.output_dir / "htdemucs_ft" / filename
        vocals_path = out_subdir / "vocals.wav"
        instrumental_path = out_subdir / "no_vocals.wav"
        if not vocals_path.exists():
            raise FileNotFoundError(f"Vocals file not found: {vocals_path}")
        if not instrumental_path.exists():
            raise FileNotFoundError(f"Instrumental file not found: {instrumental_path}")
        return vocals_path, instrumental_path

    def run(self):
        self.prepare_debug_dir()
        self.install_signal_handlers()
        try:
            self.run_demucs()
            self.write_progress(45, "人声分离完成", "separation_complete")
            vocals_path, instrumental_path = self.validate_outputs()
            self.write_progress(100, "人声分离完成", "complete")
            payload = self.result_payload(
                True,
                extra={
                    "vocals": str(vocals_path),
                    "instrumental": str(instrumental_path),
                },
            )
            self.write_result(payload)
            self.cleanup_debug_on_success()
            return 0
        except KeyboardInterrupt:
            self.demucs_returncode = 130
            exc_text = traceback.format_exc()
            self.write_traceback(exc_text)
            payload = self.result_payload(
                False,
                error="Demucs 分离已取消",
                error_code="CANCELLED",
                stage="cancelled",
            )
            self.write_result(payload)
            return 130
        except SystemExit as exc:
            code = exc.code
            if code is None:
                code = 0
            if isinstance(code, int) and code == 0:
                try:
                    vocals_path, instrumental_path = self.validate_outputs()
                    payload = self.result_payload(
                        True,
                        extra={
                            "vocals": str(vocals_path),
                            "instrumental": str(instrumental_path),
                        },
                    )
                    self.write_result(payload)
                    self.cleanup_debug_on_success()
                    return 0
                except Exception:
                    exc_text = traceback.format_exc()
                    self.write_traceback(exc_text)
                    payload = self.result_payload(
                        False,
                        error=f"Demucs 输出校验失败: {exc_text.strip().splitlines()[-1]}",
                        error_code="OUTPUT_VALIDATION_FAILED",
                        stage="output_validation",
                    )
                    self.write_result(payload)
                    return 1
            self.demucs_returncode = int(code) if isinstance(code, int) else 1
            exc_text = traceback.format_exc()
            self.write_traceback(exc_text)
            payload = self.result_payload(
                False,
                error=f"Demucs 分离失败: SystemExit({code})",
                error_code="DEMUCS_FAILED",
                stage="separating",
            )
            self.write_result(payload)
            return self.demucs_returncode or 1
        except Exception as exc:
            exc_text = traceback.format_exc()
            self.write_traceback(exc_text)
            payload = self.result_payload(
                False,
                error=f"Demucs 分离失败: {type(exc).__name__}: {exc}",
                error_code="DEMUCS_FAILED",
                stage="separating",
            )
            self.write_result(payload)
            return 1


def main():
    if len(sys.argv) != 2:
        print("Usage: demucs_worker.py separator_job.json", file=sys.stderr)
        return 2
    worker = Worker(sys.argv[1])
    return worker.run()


if __name__ == "__main__":
    raise SystemExit(main())
