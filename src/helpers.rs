use cosmwasm_std::{ensure, BankMsg, Coin, DepsMut, MessageInfo, Uint128};
use std::collections::HashMap;

use crate::error::ContractError;
use crate::state::Distribution;

pub fn validate_distribution(
    admin: &str,
    info: &MessageInfo,
    assets: &[Coin],
    nft_count: &Uint128,
) -> Result<(), ContractError> {
    ensure!(
        info.sender.as_str() == admin,
        ContractError::Unauthorized {}
    );

    let asset_map: HashMap<&str, Uint128> = assets
        .iter()
        .map(|asset_coin| (asset_coin.denom.as_str(), asset_coin.amount * nft_count))
        .collect();

    ensure!(
        !info.funds.is_empty() && !assets.is_empty() && assets.len() == info.funds.len(),
        ContractError::InvalidFundsReceived {}
    );

    ensure!(
        info.funds.iter().all(|fund_coin| {
            asset_map
                .get(fund_coin.denom.as_str())
                .map(|expected_amount| fund_coin.amount == *expected_amount)
                .unwrap_or(false)
        }),
        ContractError::InvalidDistributionInputs {}
    );

    Ok(())
}

pub struct ToPay {
    pub found: Vec<String>,
    pub not_found: Vec<String>,
}

pub fn query_owned_tokens(
    deps: &DepsMut,
    nft_address: &String,
    owner: &str,
    token_ids: Vec<String>,
    start_after: Option<String>,
    bulk: bool,
    minimum_nfts_to_claim: u64,
) -> Result<ToPay, ContractError> {
    let tokens_response: cw721::TokensResponse = deps.querier.query_wasm_smart(
        nft_address,
        &sg721_base::QueryMsg::Tokens {
            owner: owner.to_string(),
            start_after: None,
            limit: Some(minimum_nfts_to_claim as u32),
        },
    )?;
    // This approach ensure that someone could've claimed more than once
    ensure!(
        tokens_response.tokens.len() >= minimum_nfts_to_claim as usize,
        ContractError::NotEnoughNftsToClaim {}
    );

    let mut all_tokens = Vec::with_capacity(token_ids.len());
    if bulk {
        let mut start_roll: Option<String> = if let Some(start) = &start_after {
            Some(start.clone())
        } else {
            None
        };
        // Will try to catch most of them from querying Tokens (max of 5 queries)
        for _query_id in 1..=5 {
            let tokens_response: cw721::TokensResponse = deps.querier.query_wasm_smart(
                nft_address,
                &sg721_base::QueryMsg::Tokens {
                    owner: owner.to_string(),
                    start_after: start_roll.clone(),
                    limit: Some(100u32),
                },
            )?;
            if tokens_response.tokens.len() > 0 {
                start_roll = Some(tokens_response.tokens.last().unwrap().to_string());
            } else {
                break;
            }
            all_tokens.extend(tokens_response.tokens);
        }

        let (intersection_of_tokens, missing_tokens): (Vec<String>, Vec<String>) = token_ids
            .iter()
            .cloned()
            .partition(|item| all_tokens.contains(item));

        if intersection_of_tokens.len() < 1 {
            Err(ContractError::NothingToClaim {})
        } else {
            Ok(ToPay {
                found: intersection_of_tokens,
                not_found: missing_tokens,
            })
        }
    } else {
        let mut not_found_tokens = Vec::with_capacity(token_ids.len());
        for tok_id in token_ids.iter() {
            let owner_response: cw721::OwnerOfResponse = deps.querier.query_wasm_smart(
                nft_address,
                &sg721_base::QueryMsg::OwnerOf {
                    token_id: tok_id.to_string(),
                    include_expired: None,
                },
            )?;
            if owner_response.owner == owner {
                all_tokens.push(tok_id.to_string());
            } else {
                not_found_tokens.push(tok_id.to_string())
            }
        }
        if all_tokens.len() < 1 {
            Err(ContractError::NothingToClaim {})
        } else {
            Ok(ToPay {
                found: all_tokens,
                not_found: not_found_tokens,
            })
        }
    }
}

pub fn create_send_assets_messages(
    distribution: &Distribution,
    recipient: &str,
    dist_count: u64,
) -> Vec<BankMsg> {
    distribution
        .assets
        .iter()
        .map(|dist| BankMsg::Send {
            to_address: recipient.to_string(),
            amount: vec![Coin {
                denom: dist.denom.clone(),
                amount: dist.amount * Uint128::new(dist_count as u128),
            }],
        })
        .collect()
}
