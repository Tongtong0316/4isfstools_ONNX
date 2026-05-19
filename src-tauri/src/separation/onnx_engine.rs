use crate::models::SeparationEngineHealth;

use super::engine::{ProviderStrategy, SeparationEngine, SeparationEngineKind};
use super::legacy_demucs;
use super::model_registry::{ModelRegistry, HIGH_QUALITY_ONNX_MODEL_ID};

#[derive(Debug, Clone)]
pub(crate) struct OnnxSeparationEngine {
    registry: ModelRegistry,
    provider_strategy: ProviderStrategy,
}

impl OnnxSeparationEngine {
    pub(crate) fn new(registry: ModelRegistry) -> Self {
        Self {
            registry,
            provider_strategy: ProviderStrategy::for_current_platform(),
        }
    }

    pub(crate) fn health(&self) -> SeparationEngineHealth {
        let default_model_ready = self
            .registry
            .default_model()
            .map(|model| self.registry.model_ready(model))
            .unwrap_or(false);
        let high_quality_model_ready = self
            .registry
            .high_quality_model()
            .map(|model| self.registry.model_ready(model))
            .unwrap_or(false);

        SeparationEngineHealth {
            active_engine: self.kind().as_str().to_string(),
            legacy_fallback_engine: legacy_demucs::LEGACY_ENGINE_ID.to_string(),
            requested_providers: self.provider_strategy.requested_provider_names(),
            selected_provider: self
                .provider_strategy
                .fallback_provider
                .onnx_name()
                .to_string(),
            provider_fallback_reason: Some(
                "ONNX Runtime API wiring is planned; Phase ONNX-A exposes provider policy only"
                    .to_string(),
            ),
            default_model_id: self
                .registry
                .default_model()
                .map(|model| model.id.to_string())
                .unwrap_or_default(),
            default_model_ready,
            high_quality_model_id: Some(HIGH_QUALITY_ONNX_MODEL_ID.to_string()),
            high_quality_model_ready,
            onnxruntime_available: false,
            legacy_demucs_available: false,
        }
    }
}

impl SeparationEngine for OnnxSeparationEngine {
    fn kind(&self) -> SeparationEngineKind {
        SeparationEngineKind::Onnx
    }

    fn provider_strategy(&self) -> ProviderStrategy {
        self.provider_strategy.clone()
    }
}
