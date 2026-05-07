use rig::{
    embeddings::EmbeddingModel,
    vector_store::in_memory_store::InMemoryVectorStore,
};

pub struct AppVectorStore {
    store: InMemoryVectorStore<String>,
}

impl AppVectorStore {
    pub fn new_in_memory() -> Self {
        Self {
            store: InMemoryVectorStore::default(),
        }
    }

    pub fn index<M: EmbeddingModel + Clone + 'static>(
        self,
        model: M,
    ) -> rig::vector_store::in_memory_store::InMemoryVectorIndex<M, String> {
        self.store.index(model)
    }

}
