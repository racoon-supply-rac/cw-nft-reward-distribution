use cosmwasm_std::{
    ensure, entry_point, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, StdResult,
};
use cw2::set_contract_version;
use sg_std::Response;

use crate::error::ContractError;
use crate::executes::{add_distribution, claim_rewards, return_unclaimed};
use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::state::{Config, Distribution, CLAIMS, CONFIG, DISTRIBUTION};

pub const CONTRACT_NAME: &str = "crates.io:cw-nft-reward-distribution";
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    init_msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let nft_count: cw721::NumTokensResponse = deps
        .querier
        .query_wasm_smart(&init_msg.nft_address, &sg721_base::QueryMsg::NumTokens {})?;

    let config = Config {
        admin: deps.api.addr_validate(&init_msg.admin)?,
        distributor: deps.api.addr_validate(&init_msg.distributor)?,
        current_dist_id: 0,
        nft_address: deps.api.addr_validate(&init_msg.nft_address)?,
        nft_count: nft_count.count,
        current_dist_end_time: None,
        current_dist_halted: false,
    };

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::BulkClaim {
            token_ids,
            start_after,
        } => claim_rewards(deps, env, info, token_ids, start_after, true),
        ExecuteMsg::ClaimFive { token_ids } => {
            claim_rewards(deps, env, info, token_ids, None, false)
        }
        ExecuteMsg::Distribute {
            assets_per_nft,
            distribution_end_time,
            unclaimed_to_distributor,
            minimum_nfts_to_claim,
        } => add_distribution(
            deps,
            env,
            info,
            assets_per_nft,
            distribution_end_time,
            unclaimed_to_distributor,
            minimum_nfts_to_claim,
        ),
        ExecuteMsg::ReturnUnclaimed {} => return_unclaimed(deps, env, info),
        ExecuteMsg::HaltDistribution {} => {
            let mut config = CONFIG.load(deps.storage)?;
            ensure!(info.sender == config.admin, ContractError::Unauthorized {});
            config.current_dist_halted = !config.current_dist_halted;
            CONFIG.save(deps.storage, &config)?;
            Ok(Response::default())
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetConfig {} => to_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::GetIfClaimed { token_id } => {
            to_binary(&CLAIMS.load(deps.storage, &token_id).unwrap_or(false))
        }
        QueryMsg::GetCurrentDistributionInfo {} => {
            to_binary(&DISTRIBUTION.load(deps.storage).unwrap_or(Distribution {
                assets: vec![],
                claimed: 0,
                unclaimed_to_distributor: false,
                unclaimed_sent_to_distributor: None,
                minimum_nfts_to_claim: 0,
            }))
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
}
