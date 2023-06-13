use core::marker::PhantomData;

use starknet_rs::{
    business_logic::state::{
        state_api::{
            State,
            StateReader
        },
        state_cache::StorageEntry,
    },
    core::errors::state_errors::StateError,
    services::api::contract_classes::{
        compiled_class::CompiledClass,
        deprecated_contract_class::ContractClass,
    },
    // queried lc team to fix this
    // storage::dict_storage::{DictStorage, StorageKey},
    utils::{
        Address,
        ClassHash,
        CompiledClassHash
    },
};
use std::collections::HashMap;
pub type StorageKey = (Prefix, ClassHash);

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DictStorage {
    storage: HashMap<StorageKey, Vec<u8>>,
}

impl DictStorage {
    pub fn new() -> Self {
        DictStorage {
            storage: HashMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Copy)]
pub enum Prefix {
    Int,
    Float,
    Str,
    ContractState,
    ContractClass,
}

use cairo_vm::felt::Felt252;

use mp_starknet::execution::types::{ClassHashWrapper, ContractAddressWrapper, ContractClassWrapper, Felt252Wrapper};
use mp_starknet::state::StateChanges;
use sp_std::sync::Arc;
use starknet_api::api_core::Nonce;
use starknet_api::hash::StarkFelt;
use starknet_api::state::StateDiff;

use crate::alloc::string::ToString;
use crate::types::{ContractStorageKeyWrapper, StorageKeyWrapper};
use crate::{Config, Pallet};

/// Empty struct that implements the traits needed by the blockifier/starknet in rust.
///
/// We feed this struct when executing a transaction so that we directly use the substrate storage
/// and not an extra layer that would add overhead.
/// We don't implement those traits directly on the pallet to avoid compilation problems.
pub struct StarknetStateAdapter<T: Config> {
    storage_update: DictStorage,
    class_hash_update: usize,
    _phantom: PhantomData<T>,
}

impl<T> StateChanges for StarknetStateAdapter<T>
where
    T: Config,
{
    fn count_state_changes(&self) -> (usize, usize, usize) {
        let keys = self.storage_update.storage.keys();
        let n_contract_updated = keys.into_iter().map(|&(contract_address, _)| contract_address).len();
        (n_contract_updated, keys.len(), self.class_hash_update)
    }
}

impl<T: Config> Default for StarknetStateAdapter<T> {
    fn default() -> Self {
        Self { storage_update: DictStorage::new(), class_hash_update: usize::default(), _phantom: PhantomData }
    }
}

impl<T: Config> StateReader for StarknetStateAdapter<T> {
    fn get_contract_class(&mut self, class_hash: &ClassHash) -> Result<CompiledClass, StateError> {
        let wrapped_class_hash: ClassHashWrapper = ClassHashWrapper::from(class_hash.into());
        let opt_contract_class = Pallet::<T>::contract_class_by_class_hash(wrapped_class_hash);
        match opt_contract_class {
            Some(contract_class) => Ok(Arc::new(
                TryInto::<ContractClass>::try_into(contract_class)
                    .map_err(|e| StateError::StateReadError(e.to_string()))?,
            )),
            None => Err(StateError::UndeclaredClassHash(*class_hash)),
        }
    }

    fn get_class_hash_at(&mut self, contract_address: &Address) -> Result<ClassHash, StateError> {
        let contract_address: ContractAddressWrapper = ContractAddressWrapper::from(contract_address.0.into());

        let class_hash = ClassHash(StarkFelt::new(
            Pallet::<T>::contract_class_hash_by_address(contract_address).unwrap_or_default().into(),
        )?);

        Ok(class_hash)
    }

    fn get_nonce_at(&mut self, contract_address: &Address) -> Result<StarkFelt, StateError> {
        let contract_address: ContractAddressWrapper = ContractAddressWrapper::from(contract_address.0.into());
        let nonce = Nonce(StarkFelt::new(Pallet::<T>::nonce(contract_address).into())?);

        Ok(nonce)
    }

    fn get_storage_at(&mut self, storage_entry: &StorageEntry) -> Result<StarkFelt, StateError> {
        let contract_address: ContractAddressWrapper = ContractAddressWrapper::from(storage_entry.0.0.into());
        let key: StorageKeyWrapper = StorageKeyWrapper::from(storage_entry.1.into());

        let contract_storage_key: ContractStorageKeyWrapper = (contract_address, key);
        let storage_content = StarkFelt::new(Pallet::<T>::storage(contract_storage_key).into())?;

        Ok(storage_content)
    }

    fn get_compiled_class_hash(
            &mut self,
            class_hash: &ClassHash,
        ) -> Result<CompiledClassHash, StateError> {
        
        Ok(())
    }
}

impl<T: Config> State for StarknetStateAdapter<T> {
    fn apply_state_update(&mut self, state_updates: &StateDiff) -> Result<(), StateError> {
        Ok(())
    }

    fn count_actual_storage_changes(&mut self) -> (usize, usize) {
        (1, 1)
    }

    fn deploy_contract(
        &mut self,
        contract_address: Address,
        class_hash: ClassHash,
    ) -> Result<(), StateError> {
        Ok(())
    }

    fn increment_nonce(&mut self, contract_address: &Address) -> Result<(), StateError> {
        let contract_address: ContractAddressWrapper = ContractAddressWrapper::from(contract_address.0.into());
        let current_nonce = Pallet::<T>::nonce(contract_address);

        crate::Nonces::<T>::insert(contract_address, current_nonce + 1);

        Ok(())
    }

    fn set_compiled_class(
        &mut self,
        compiled_class_hash: &StarkFelt,
        casm_class: CasmContractClass,
    ) -> Result<(), StateError> {
        Ok(())
    }

    fn set_compiled_class_hash(
        &mut self,
        class_hash: &StarkFelt,
        compiled_class_hash: &StarkFelt,
    ) -> Result<(), StateError> {
        Ok(())
    }

    fn set_contract_class(&mut self, class_hash: &ClassHash, contract_class: ContractClass) -> Result<(), StateError> {
        let class_hash: ClassHashWrapper = class_hash.0.into();
        let contract_class: ContractClassWrapper = ContractClassWrapper::try_from(contract_class).unwrap();

        crate::ContractClasses::<T>::insert(class_hash, contract_class);

        Ok(())
    }

    fn set_storage_at(&mut self, storage_entry: &StorageEntry, value: Felt252) {
        self.storage_update.storage.insert((contract_address, key), value);
        let contract_address: ContractAddressWrapper = ContractAddressWrapper::from(contract_address.0.0.into());
        let key: StorageKeyWrapper = key.0.0.into();

        let contract_storage_key: ContractStorageKeyWrapper = (contract_address, key);

        crate::StorageView::<T>::insert(contract_storage_key, Felt252Wrapper::from(value));
    }

    fn set_class_hash_at(&mut self, contract_address: Address, class_hash: ClassHash) -> Result<(), StateError> {
        self.class_hash_update += 1;
        let contract_address: ContractAddressWrapper = ContractAddressWrapper::from(contract_address.0.into());
        let class_hash: ClassHashWrapper = ClassHashWrapper::from(class_hash.into());

        crate::ContractClassHashes::<T>::insert(contract_address, class_hash);

        Ok(())
    }
}
