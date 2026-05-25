use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SeparationEngineKind {
    Onnx,
}

impl SeparationEngineKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Onnx => "onnx",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ExecutionProvider {
    Dml,
    CoreMl,
    Cpu,
}

impl ExecutionProvider {
    pub(crate) fn onnx_name(self) -> &'static str {
        match self {
            Self::Dml => "DmlExecutionProvider",
            Self::CoreMl => "CoreMLExecutionProvider",
            Self::Cpu => "CPUExecutionProvider",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ProviderStrategy {
    pub(crate) requested_providers: Vec<ExecutionProvider>,
    pub(crate) fallback_provider: ExecutionProvider,
}

impl ProviderStrategy {
    pub(crate) fn for_current_platform() -> Self {
        #[cfg(target_os = "windows")]
        {
            return Self {
                requested_providers: vec![ExecutionProvider::Dml, ExecutionProvider::Cpu],
                fallback_provider: ExecutionProvider::Cpu,
            };
        }

        #[cfg(target_os = "macos")]
        {
            return Self {
                requested_providers: vec![ExecutionProvider::CoreMl, ExecutionProvider::Cpu],
                fallback_provider: ExecutionProvider::Cpu,
            };
        }

        #[allow(unreachable_code)]
        Self {
            requested_providers: vec![ExecutionProvider::Cpu],
            fallback_provider: ExecutionProvider::Cpu,
        }
    }

    pub(crate) fn requested_provider_names(&self) -> Vec<String> {
        self.requested_providers
            .iter()
            .map(|provider| provider.onnx_name().to_string())
            .collect()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SeparationJobInput {
    pub(crate) song_id: String,
    pub(crate) input_path: String,
    pub(crate) output_dir: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SeparationJobOutput {
    pub(crate) vocals_path: String,
    pub(crate) instrumental_path: String,
}

pub(crate) trait SeparationEngine {
    fn kind(&self) -> SeparationEngineKind;
    fn provider_strategy(&self) -> ProviderStrategy;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_strategy_always_has_cpu_fallback() {
        let strategy = ProviderStrategy::for_current_platform();
        assert_eq!(strategy.fallback_provider, ExecutionProvider::Cpu);
        assert!(strategy
            .requested_providers
            .contains(&ExecutionProvider::Cpu));
    }
}
