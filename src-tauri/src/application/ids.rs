pub(crate) trait IdGenerator: Send + Sync {
    fn next_id(&self) -> String;
}

pub(crate) struct UuidV7Generator;

impl IdGenerator for UuidV7Generator {
    fn next_id(&self) -> String {
        uuid::Uuid::now_v7().to_string()
    }
}
