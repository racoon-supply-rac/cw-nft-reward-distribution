use cosmwasm_std::{ensure, BankMsg, Coin, DepsMut, Env, MessageInfo, Timestamp, Uint128};
use sg_std::Response;

use crate::error::ContractError;
use crate::helpers::{create_send_assets_messages, query_owned_tokens, validate_distribution};
use crate::state::{Distribution, CLAIMS, CONFIG, DISTRIBUTION};

pub fn claim_rewards(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    token_ids: Vec<String>,
    start_after: Option<String>,
    bulk: bool,
) -> Result<Response, ContractError> {
    // Bulk uses the "Tokens" query approach
    if bulk {
        if token_ids.len() > 500 {
            return Err(ContractError::TooManyTokensSent {});
        }
    // ClaimFive approach queries each token id
    } else {
        if token_ids.len() > 5 {
            return Err(ContractError::TooManyTokensSent {});
        }
    }

    let config = CONFIG.load(deps.storage)?;
    // If no Distribution logged -> this will error
    let mut distribution = DISTRIBUTION.load(deps.storage)?;

    if config.current_dist_halted {
        return Err(ContractError::DistributionHalted {});
    }

    let dist_timer_end = config
        .current_dist_end_time
        .unwrap_or(Timestamp::from_seconds(0u64));
    if env.block.time > dist_timer_end {
        return Err(ContractError::ClaimingWindowHasClosed {});
    }

    // Intersection between provided and actually owned
    let validated_tokens = query_owned_tokens(
        &deps,
        &config.nft_address.to_string(),
        info.sender.as_ref(),
        token_ids,
        start_after,
        bulk,
        distribution.minimum_nfts_to_claim,
    )?;
    let owned_tokens = validated_tokens.found;

    let mut dist_count: u64 = 0;
    for token in owned_tokens.iter() {
        // If it errors -> means unclaimed
        if CLAIMS.load(deps.storage, token).is_err() {
            dist_count += 1;
            CLAIMS.save(deps.storage, token.as_str(), &true)?;
        }
    }

    if dist_count < 1 {
        return Err(ContractError::NothingToClaim {});
    }

    distribution.claimed += dist_count;

    DISTRIBUTION.save(deps.storage, &distribution)?;

    let messages = create_send_assets_messages(&distribution, info.sender.as_str(), dist_count);
    let mut response = Response::new().add_messages(messages);
    if !validated_tokens.not_found.is_empty() {
        // Returns an answer if the required tokens were owned or not
        response = response.add_attribute(
            "Tokens not found or not owned: ".to_string(),
            validated_tokens.not_found.join(", "),
        );
    }
    Ok(response)
}

pub fn return_unclaimed(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    // Returns the unclaimed amounts to to distributor if it wasnt entirely claimed
    // Need the distribution to have ended
    let config = CONFIG.load(deps.storage)?;
    ensure!(
        !config.current_dist_halted,
        ContractError::DistributionHalted {}
    );
    ensure!(
        env.block.time > config.current_dist_end_time.unwrap(),
        ContractError::CurrentDistributionHasNotYetEnded {}
    );
    ensure!(
        config.distributor == info.sender,
        ContractError::Unauthorized {}
    );

    let mut current_dist = DISTRIBUTION.load(deps.storage)?;
    ensure!(
        current_dist.unclaimed_to_distributor,
        ContractError::CurrentUnclaimedGoesToTheSubsequentDistribution {}
    );
    ensure!(
        !current_dist.unclaimed_sent_to_distributor.unwrap(),
        ContractError::UnclaimedWasAlreadyReturned {}
    );
    ensure!(
        current_dist.claimed < config.nft_count,
        ContractError::NothingToReturn {}
    );

    // Can return the unclaimed
    let remaining_to_dist = config.nft_count - current_dist.claimed;
    let mut response = Response::new();
    for curr_dist in &current_dist.assets {
        let remain_contract = deps
            .querier
            .query_balance(env.contract.address.as_str(), &curr_dist.denom)?;
        ensure!(
            remain_contract.amount >= Uint128::new(remaining_to_dist as u128) * curr_dist.amount,
            ContractError::InvalidClaimValue {}
        );
        response = response.add_message(BankMsg::Send {
            to_address: config.distributor.to_string(),
            amount: vec![Coin {
                denom: remain_contract.denom.clone(),
                amount: Uint128::new(remaining_to_dist as u128) * curr_dist.amount,
            }],
        });
    }

    current_dist.unclaimed_sent_to_distributor = Some(true);
    DISTRIBUTION.save(deps.storage, &current_dist)?;

    Ok(response)
}

pub fn add_distribution(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    mut assets_per_nft: Vec<Coin>,
    distribution_end_time: Timestamp,
    unclaimed_to_distributor: bool,
    minimum_nfts_to_claim: u64,
) -> Result<Response, ContractError> {
    ensure!(
        distribution_end_time > env.block.time,
        ContractError::InvalidDistributionEndTime {}
    );
    let mut config = CONFIG.load(deps.storage)?;
    if config.current_dist_halted {
        return Err(ContractError::DistributionHalted {});
    }
    if config.current_dist_end_time.is_some() {
        let prev_dist = config.current_dist_end_time.unwrap();
        ensure!(
            prev_dist < env.block.time,
            ContractError::CurrentDistributionHasNotYetEnded {}
        );
        ensure!(
            prev_dist < distribution_end_time,
            ContractError::InvalidDistributionEndTime {}
        );
    }
    let previous_dist = DISTRIBUTION
        .load(deps.storage)
        .unwrap_or_else(|_| Distribution {
            assets: vec![],
            claimed: 0,
            unclaimed_to_distributor: false,
            unclaimed_sent_to_distributor: None,
            minimum_nfts_to_claim: 0,
        });

    validate_distribution(
        config.distributor.as_str(),
        &info,
        &assets_per_nft,
        &Uint128::new(config.nft_count as u128),
    )?;

    config.current_dist_id += 1;
    config.current_dist_end_time = Some(distribution_end_time);

    let remaining_to_dist = config.nft_count - previous_dist.claimed;

    let mut new_assets_per_nft: Vec<Coin> = Vec::with_capacity(assets_per_nft.len());

    let mut response: Response = Response::new();

    for prev_dist in &previous_dist.assets {
        let remain_contract = deps
            .querier
            .query_balance(env.contract.address.as_str(), &prev_dist.denom)?;
        let zero_coin = &Coin {
            denom: prev_dist.denom.clone(),
            amount: Uint128::zero(),
        };
        let being_sent_validated = info
            .funds
            .iter()
            .find(|coin| coin.denom == prev_dist.denom)
            .unwrap_or(zero_coin);
        let rem_amount = remain_contract.amount - being_sent_validated.amount;
        if previous_dist.unclaimed_to_distributor
            && previous_dist.unclaimed_sent_to_distributor == Some(false)
        {
            response = response.add_message(BankMsg::Send {
                to_address: config.distributor.to_string(),
                amount: vec![Coin {
                    denom: remain_contract.denom.clone(),
                    amount: Uint128::new(remaining_to_dist as u128) * prev_dist.amount,
                }],
            });
        }
        if !previous_dist.unclaimed_to_distributor {
            ensure!(
                rem_amount >= Uint128::new(remaining_to_dist as u128) * prev_dist.amount,
                ContractError::InvalidDistributionInputs {}
            );
            let mut found_asset = false;
            for curr_dist in &mut assets_per_nft {
                if curr_dist.denom == prev_dist.denom {
                    curr_dist.amount += (Uint128::new(remaining_to_dist as u128)
                        * prev_dist.amount)
                        / Uint128::new(config.nft_count as u128);
                    found_asset = true;
                    break;
                }
            }

            if !found_asset {
                new_assets_per_nft.push(Coin {
                    denom: prev_dist.denom.clone(),
                    amount: (Uint128::new(remaining_to_dist as u128) * prev_dist.amount)
                        / Uint128::new(config.nft_count as u128),
                });
            }
        }
    }

    if !unclaimed_to_distributor {
        assets_per_nft.extend(new_assets_per_nft);
    }

    DISTRIBUTION.save(
        deps.storage,
        &Distribution {
            assets: assets_per_nft.clone(),
            claimed: 0,
            unclaimed_to_distributor,
            unclaimed_sent_to_distributor: if unclaimed_to_distributor {
                Some(false)
            } else {
                None
            },
            minimum_nfts_to_claim,
        },
    )?;

    // Remove all states
    CLAIMS.clear(deps.storage);

    CONFIG.save(deps.storage, &config)?;

    Ok(response)
}
