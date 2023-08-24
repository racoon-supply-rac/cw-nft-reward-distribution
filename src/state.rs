use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Coin, Timestamp};
use cw_storage_plus::{Item, Map};

#[cw_serde]
pub struct Config {
    pub admin: Addr,
    pub distributor: Addr,
    pub current_dist_id: u64,
    pub nft_address: Addr,
    pub nft_count: u64,
    pub current_dist_end_time: Option<Timestamp>,
    pub current_dist_halted: bool,
}

pub const CONFIG: Item<Config> = Item::new("config");

#[cw_serde]
pub struct Distribution {
    pub assets: Vec<Coin>,
    pub claimed: u64,
    pub unclaimed_to_distributor: bool,
    pub unclaimed_sent_to_distributor: Option<bool>,
    pub minimum_nfts_to_claim: u64,
}

pub const DISTRIBUTION: Item<Distribution> = Item::new("distribution");

pub const CLAIMS: Map<&str, bool> = Map::new("claims");
