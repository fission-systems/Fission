use super::*;

#[derive(Debug)]
struct BuilderStatsProjection<'a> {
    telemetry: &'a super::telemetry::BuilderTelemetry,
    validated_pcode_op_count: usize,
}

impl<'a> BuilderStatsProjection<'a> {
    fn from_builder<'builder>(builder: &'a PreviewBuilder<'builder>) -> Self {
        Self {
            telemetry: &builder.telemetry,
            validated_pcode_op_count: builder
                .pcode
                .blocks
                .iter()
                .map(|block| block.ops.len())
                .sum(),
        }
    }

    fn into_public_stats(self) -> PreviewBuildStats {
        let mut stats = PreviewBuildStats {
            validated_pcode_op_count: self.validated_pcode_op_count,
            ..PreviewBuildStats::default()
        };
        self.telemetry.apply_to_public_stats(&mut stats);
        stats
    }
}

impl<'a> PreviewBuilder<'a> {
    pub(crate) fn preview_build_stats(&self) -> PreviewBuildStats {
        BuilderStatsProjection::from_builder(self).into_public_stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_stats_projection_keeps_public_nir_build_stats_field_set() {
        let telemetry = super::super::telemetry::BuilderTelemetry::default();
        let default_keys = serialized_keys(&PreviewBuildStats::default());
        let projected_keys = serialized_keys(
            &BuilderStatsProjection {
                telemetry: &telemetry,
                validated_pcode_op_count: 17,
            }
            .into_public_stats(),
        );
        assert_eq!(projected_keys, default_keys);
    }

    fn serialized_keys(stats: &PreviewBuildStats) -> Vec<String> {
        let serde_json::Value::Object(object) =
            serde_json::to_value(stats).expect("serialize NirBuildStats")
        else {
            panic!("NirBuildStats must serialize as an object");
        };
        object.keys().cloned().collect()
    }
}
