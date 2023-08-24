use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Coin, Timestamp};

#[cw_serde]
pub struct InstantiateMsg {
    pub admin: String,
    pub distributor: String,
    pub nft_address: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    BulkClaim {
        token_ids: Vec<String>,
        start_after: Option<String>,
    },
    ClaimFive {
        token_ids: Vec<String>,
    },
    Distribute {
        assets_per_nft: Vec<Coin>,
        distribution_end_time: Timestamp,
        unclaimed_to_distributor: bool,
        minimum_nfts_to_claim: u64,
    },
    HaltDistribution {},
    ReturnUnclaimed {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(crate::state::Config)]
    GetConfig {},
    #[returns(crate::state::Distribution)]
    GetCurrentDistributionInfo {},
    #[returns(bool)]
    GetIfClaimed { token_id: String },
}

#[cw_serde]
pub struct MigrateMsg {}
