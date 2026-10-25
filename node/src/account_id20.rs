//! Consistent AccountId20 mapping (must match genesis `get_account_id_from_seed`).

use fp_account::AccountId20;
use crate::chain_spec;

pub fn from_secret_uri(uri: &str) -> AccountId20 {
    chain_spec::get_account_id_from_seed(uri.trim())
}
