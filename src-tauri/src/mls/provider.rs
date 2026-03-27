use openmls_rust_crypto::RustCrypto;
use openmls_traits::OpenMlsProvider;

use crate::mls::storage::SqlCipherMlsStorage;

pub struct HaexMlsProvider {
    crypto: RustCrypto,
    storage: SqlCipherMlsStorage,
}

impl HaexMlsProvider {
    pub fn new(storage: SqlCipherMlsStorage) -> Self {
        Self {
            crypto: RustCrypto::default(),
            storage,
        }
    }
}

impl OpenMlsProvider for HaexMlsProvider {
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type StorageProvider = SqlCipherMlsStorage;

    fn storage(&self) -> &Self::StorageProvider {
        &self.storage
    }

    fn crypto(&self) -> &Self::CryptoProvider {
        &self.crypto
    }

    fn rand(&self) -> &Self::RandProvider {
        &self.crypto
    }
}
