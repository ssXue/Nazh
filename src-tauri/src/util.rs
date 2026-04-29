use nazh_engine::EngineError;

pub(crate) fn stringify_error(error: &EngineError) -> String {
    error.to_string()
}
