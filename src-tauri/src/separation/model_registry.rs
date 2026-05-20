use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub(crate) const DEFAULT_ONNX_MODEL_ID: &str = "uvr_mdxnet_9482";
pub(crate) const DEFAULT_ONNX_MODEL_FILENAME: &str = "UVR_MDXNET_9482.onnx";
pub(crate) const HIGH_QUALITY_ONNX_MODEL_ID: &str = "uvr_mdx_net_inst_hq_5";
pub(crate) const HIGH_QUALITY_ONNX_MODEL_FILENAME: &str = "UVR-MDX-NET-Inst_HQ_5.onnx";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ModelInstallMode {
    BundledDefault,
    OptionalHighQuality,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SeparationModel {
    pub(crate) id: &'static str,
    pub(crate) display_name: &'static str,
    pub(crate) filename: &'static str,
    pub(crate) install_mode: ModelInstallMode,
}

impl SeparationModel {
    pub(crate) fn path_under(&self, models_dir: &Path) -> PathBuf {
        models_dir.join("onnx").join(self.filename)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ModelRegistry {
    models_dir: PathBuf,
    models: Vec<SeparationModel>,
}

impl ModelRegistry {
    pub(crate) fn from_models_dir(models_dir: &Path) -> Self {
        Self {
            models_dir: models_dir.to_path_buf(),
            models: vec![
                SeparationModel {
                    id: DEFAULT_ONNX_MODEL_ID,
                    display_name: "UVR MDXNET 9482",
                    filename: DEFAULT_ONNX_MODEL_FILENAME,
                    install_mode: ModelInstallMode::BundledDefault,
                },
                SeparationModel {
                    id: HIGH_QUALITY_ONNX_MODEL_ID,
                    display_name: "UVR MDX-NET Inst_HQ_5",
                    filename: HIGH_QUALITY_ONNX_MODEL_FILENAME,
                    install_mode: ModelInstallMode::OptionalHighQuality,
                },
            ],
        }
    }

    pub(crate) fn default_model(&self) -> Option<&SeparationModel> {
        self.models
            .iter()
            .find(|model| model.install_mode == ModelInstallMode::BundledDefault)
    }

    pub(crate) fn high_quality_model(&self) -> Option<&SeparationModel> {
        self.models
            .iter()
            .find(|model| model.install_mode == ModelInstallMode::OptionalHighQuality)
    }

    pub(crate) fn model_ready(&self, model: &SeparationModel) -> bool {
        model.path_under(&self.models_dir).exists()
    }

    pub(crate) fn default_model_path(&self) -> Option<PathBuf> {
        self.default_model()
            .map(|model| model.path_under(&self.models_dir))
    }

    pub(crate) fn high_quality_model_path(&self) -> Option<PathBuf> {
        self.high_quality_model()
            .map(|model| model.path_under(&self.models_dir))
    }
}
