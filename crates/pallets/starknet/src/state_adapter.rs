use core::marker::PhantomData;
use std::collections::HashMap;

// Blockifier imports
use blockifier::execution::contract_class::ContractClass as BlockifierContractClass;
use blockifier::state::errors::StateError as BlockifierStateError;
use blockifier::state::state_api::{State as BlockifierState, StateReader as BlockifierStateReader, StateResult as BlockifierStateResult};

// Starknet-rs imports
use starknet_rs::{
    business_logic::fact_state::state::StateDiff as StarknetStateDiff,
    business_logic::state::{
        state_api::{
            State as StarknetState,
            StateReader as StarknetStateReader,
        },
        state_cache::StorageEntry as StarknetStorageEntry,
    },
    core::errors::state_errors::StateError as StarknetStateError,
    services::api::contract_classes::{
        compiled_class::CompiledClass as StarknetCompiledClass,
        deprecated_contract_class::ContractClass as StarknetContractClass,
    },
    // queried lc team to fix this
    // storage::dict_storage::{DictStorage, StorageKey},
    utils::{
        Address as StarknetAddress,
        ClassHash as StarknetClassHash,
        CompiledClassHash as StarknetCompiledClassHash,
    },
};
pub type StarknetStorageKey = (Prefix, ClassHash);

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DictStorage {
    storage: HashMap<StarknetStorageKey, Vec<u8>>,
}

impl DictStorage {
    pub fn new() -> Self {
        DictStorage {
            storage: HashMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Copy)]
enum Prefix {
    Int,
    Float,
    Str,
    ContractState,
    ContractClass,
}

// -----

use mp_starknet::execution::types::{ClassHashWrapper, ContractAddressWrapper, ContractClassWrapper, Felt252Wrapper};
use mp_starknet::state::StateChanges;
use sp_std::sync::Arc;
use starknet_api::api_core::{ClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::{StateDiff, StorageKey};

use crate::alloc::string::ToString;
use crate::types::{ContractStorageKeyWrapper, StorageKeyWrapper};
use crate::{Config, Pallet};

/// Empty struct that implements the traits needed by the blockifier/starknet in rust.
///
/// We feed this struct when executing a transaction so that we directly use the substrate storage
/// and not an extra layer that would add overhead.
/// We don't implement those traits directly on the pallet to avoid compilation problems.
pub struct StateAdapter<T: Config> {
    storage_update: HashMap<ContractStorageKeyWrapper, Felt252Wrapper>,
    class_hash_update: usize,
    _phantom: PhantomData<T>,
}

impl<T> StateChanges for StateAdapter<T>
where
    T: Config,
{
    fn count_state_changes(&self) -> (usize, usize, usize) {
        let keys = self.storage_update.keys();
        let n_contract_updated = keys.into_iter().map(|&(contract_address, _)| contract_address).len();
        (n_contract_updated, keys.len(), self.class_hash_update)
    }
}

impl<T: Config> Default for StateAdapter<T> {
    fn default() -> Self {
        Self { storage_update: HashMap::new(), class_hash_update: usize::default(), _phantom: PhantomData }
    }
}

impl<T: Config> BlockifierStateReader for StateAdapter<T> {
    fn get_storage_at(&mut self, contract_address: ContractAddress, key: StorageKey) -> BlockifierStateResult<StarkFelt> {
        let contract_address: ContractAddressWrapper = contract_address.0.0.into();
        let key: StorageKeyWrapper = key.0.0.into();

        let contract_storage_key: ContractStorageKeyWrapper = (contract_address, key);
        let storage_content = StarkFelt::new(Pallet::<T>::storage(contract_storage_key).into())?;

        Ok(storage_content)
    }

    fn get_nonce_at(&mut self, contract_address: ContractAddress) -> BlockifierStateResult<Nonce> {
        let contract_address: ContractAddressWrapper = contract_address.0.0.into();

        let nonce = Nonce(StarkFelt::new(Pallet::<T>::nonce(contract_address).into())?);

        Ok(nonce)
    }

    fn get_class_hash_at(&mut self, contract_address: ContractAddress) -> BlockifierStateResult<ClassHash> {
        let contract_address: ContractAddressWrapper = contract_address.0.0.into();

        let class_hash = ClassHash(StarkFelt::new(
            Pallet::<T>::contract_class_hash_by_address(contract_address).unwrap_or_default().into(),
        )?);

        Ok(class_hash)
    }

    fn get_contract_class(&mut self, class_hash: &ClassHash) -> BlockifierStateResult<Arc<BlockifierContractClass>> {
        let wrapped_class_hash: ClassHashWrapper = class_hash.0.into();
        let opt_contract_class = Pallet::<T>::contract_class_by_class_hash(wrapped_class_hash);
        match opt_contract_class {
            Some(contract_class) => Ok(Arc::new(
                TryInto::<BlockifierContractClass>::try_into(contract_class)
                    .map_err(|e| BlockifierStateError::StateReadError(e.to_string()))?,
            )),
            None => Err(BlockifierStateError::UndeclaredClassHash(*class_hash)),
        }
    }
}

impl<T: Config> BlockifierState for StateAdapter<T> {
    fn set_storage_at(&mut self, contract_address: ContractAddress, key: StorageKey, value: StarkFelt) {
        self.storage_update.insert((contract_address, key), value);
        let contract_address: ContractAddressWrapper = contract_address.0.0.into();
        let key: StorageKeyWrapper = key.0.0.into();

        let contract_storage_key: ContractStorageKeyWrapper = (contract_address, key);

        crate::StorageView::<T>::insert(contract_storage_key, Felt252Wrapper::from(value));
    }

    fn increment_nonce(&mut self, contract_address: ContractAddress) -> BlockifierStateResult<()> {
        let contract_address: ContractAddressWrapper = contract_address.0.0.into();
        let current_nonce = Pallet::<T>::nonce(contract_address);

        crate::Nonces::<T>::insert(contract_address, current_nonce + 1);

        Ok(())
    }

    fn set_class_hash_at(&mut self, contract_address: ContractAddress, class_hash: ClassHash) -> BlockifierStateResult<()> {
        self.class_hash_update += 1;
        let contract_address: ContractAddressWrapper = contract_address.0.0.into();
        let class_hash: ClassHashWrapper = class_hash.0.into();

        crate::ContractClassHashes::<T>::insert(contract_address, class_hash);

        Ok(())
    }

    fn set_contract_class(&mut self, class_hash: &ClassHash, contract_class: BlockifierContractClass) -> BlockifierStateResult<()> {
        let class_hash: ClassHashWrapper = class_hash.0.into();
        let contract_class: ContractClassWrapper = ContractClassWrapper::try_from(contract_class).unwrap();

        crate::ContractClasses::<T>::insert(class_hash, contract_class);

        Ok(())
    }

    /// As the state is updated during the execution, return an empty [StateDiff]
    ///
    /// There is no reason to use it in the current implementation of the trait
    fn to_state_diff(&self) -> StateDiff {
        StateDiff::default()
    }
}




impl<T: Config> StarknetStateReader for StateAdapter<T> {
    fn get_contract_class(&mut self, class_hash: &StarknetClassHash) -> Result<StarknetCompiledClass, StarknetStateError> {
        todo!()
    }

    fn get_class_hash_at(&mut self, contract_address: &StarknetAddress) -> Result<StarknetClassHash, StarknetStateError> {
        todo!()
    }

    fn get_nonce_at(&mut self, contract_address: &StarknetAddress) -> Result<Felt252Wrapper, StarknetStateError> {
        todo!()
    }

    fn get_storage_at(&mut self, storage_entry: &StarknetStorageEntry) -> Result<Felt252Wrapper, StarknetStateError> {
        todo!()
    }

    fn get_compiled_class_hash(
            &mut self,
            class_hash: &StarknetClassHash,
        ) -> Result<StarknetCompiledClassHash, StarknetStateError> {
        todo!()
    }
}

impl<T: Config> StarknetState for StateAdapter<T> {
    fn apply_state_update(&mut self, state_updates: &StarknetStateDiff) -> Result<(), StarknetStateError> {
        todo!()
    }

    fn count_actual_storage_changes(&mut self) -> (usize, usize) {
        todo!()
    }

    fn deploy_contract(
        &mut self,
        contract_address: StarknetAddress,
        class_hash: StarknetClassHash,
    ) -> Result<(), StarknetStateError> {
        todo!()
    }

    fn increment_nonce(&mut self, contract_address: &StarknetAddress) -> Result<(), StarknetStateError> {
        todo!()
    }

    fn set_compiled_class(
        &mut self,
        compiled_class_hash: &StarkFelt,
        casm_class: CasmContractClass,
    ) -> Result<(), StarknetStateError> {
        todo!()
    }

    fn set_compiled_class_hash(
        &mut self,
        class_hash: &Felt252Wrapper,
        compiled_class_hash: &StarkFelt,
    ) -> Result<(), StarknetStateError> {
        todo!()
    }

    fn set_contract_class(&mut self, class_hash: &StarknetClassHash, contract_class: &StarknetContractClass) -> Result<(), StarknetStateError> {
        todo!()
    }

    fn set_storage_at(&mut self, storage_entry: &StarknetStorageEntry, value: Felt252Wrapper) {
        todo!()
    }

    fn set_class_hash_at(&mut self, contract_address: StarknetAddress, class_hash: StarknetClassHash) -> Result<(), StarknetStateError> {
        todo!()
    }
}
