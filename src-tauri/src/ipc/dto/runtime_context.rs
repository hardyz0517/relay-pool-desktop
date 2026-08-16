use super::TypeDescriptor;

pub(crate) use crate::observability::runtime_context::{
    IpcRuntimeContextV1, RuntimeContextRegistry, RuntimeContextValidationError,
    ValidatedRuntimeContext, DEFAULT_INTERACTION_TTL, DEFAULT_MAX_ACTIVE_INTERACTIONS,
};

pub(crate) const IPC_RUNTIME_CONTEXT_TYPE: TypeDescriptor = TypeDescriptor {
    name: "IpcRuntimeContextV1",
    typescript: include_str!("runtime_context.typescript.txt"),
};
