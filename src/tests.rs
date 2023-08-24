#[cfg(test)]
mod tests {

    // Parts of the tests were taken from https://github.com/public-awesome/launchpad

    use cosmwasm_std::{coin, Addr, BlockInfo, Coin, Timestamp, Uint128};
    use cw721::TokensResponse;
    use cw_multi_test::{AppResponse, BankSudo, Contract, ContractWrapper, Executor, SudoMsg};
    use sg2::tests::mock_collection_params;
    use sg_multi_test::StargazeApp;
    use sg_std::StargazeMsgWrapper;
    use test_suite::common_setup::contract_boxes::{
        contract_sg721_base, contract_vending_factory, contract_vending_minter, custom_mock_app,
    };
    use test_suite::common_setup::setup_minter::common::constants::CREATION_FEE;
    use test_suite::common_setup::setup_minter::vending_minter::mock_params::{
        mock_create_minter, mock_params,
    };
    use vending_factory::helpers::FactoryContract;
    use vending_factory::msg::{ExecuteMsg, InstantiateMsg as FactoryInstantiateMsg};

    // Constants
    const GOVERNANCE: &str = "governance";
    const ADMIN: &str = "admin";
    const WALLET1: &str = "wallet1";
    const WALLET2: &str = "wallet2";
    const WALLET3: &str = "wallet3";
    const DISTRIBUTOR: &str = "distributor";
    const NATIVE_DENOM: &str = "ustars";
    const NATIVE_DENOM1: &str = "urac";
    const NATIVE_DENOM2: &str = "uatom";
    const NATIVE_DENOM3: &str = "uwhale";
    const NATIVE_DENOM4: &str = "urandom";

    pub fn contract_nft_reward_distribution() -> Box<dyn Contract<StargazeMsgWrapper>> {
        let contract = ContractWrapper::new(
            crate::contract::execute,
            crate::contract::instantiate,
            crate::contract::query,
        );
        Box::new(contract)
    }

    fn valid_instantiate_nft_reward_distribution(
        mut app: StargazeApp,
        nft_address: Addr,
    ) -> (StargazeApp, Addr) {
        let nft_reward_dist_id = app.store_code(contract_nft_reward_distribution());

        let nft_reward_dist_addr = app
            .instantiate_contract(
                nft_reward_dist_id,
                Addr::unchecked(ADMIN),
                &crate::msg::InstantiateMsg {
                    admin: ADMIN.to_string(),
                    distributor: DISTRIBUTOR.to_string(),
                    nft_address: nft_address.to_string(),
                },
                &[],
                "nft_reward_dist",
                Some(ADMIN.to_string()),
            )
            .unwrap();

        (app, nft_reward_dist_addr)
    }

    fn valid_instantiate_factory_mint() -> (StargazeApp, FactoryContract) {
        let mut app = custom_mock_app();
        let factory_id = app.store_code(contract_vending_factory());
        let minter_id = app.store_code(contract_vending_minter());

        let mut params = mock_params();
        params.code_id = minter_id;

        let msg = FactoryInstantiateMsg { params };
        let factory_addr = app
            .instantiate_contract(
                factory_id,
                Addr::unchecked(GOVERNANCE),
                &msg,
                &[],
                "factory",
                Some(GOVERNANCE.to_string()),
            )
            .unwrap();

        let factory_contract = FactoryContract(factory_addr);

        (app, factory_contract)
    }

    struct InitReturn {
        minter_addr: Addr,
        nft_addr: Addr,
    }

    fn valid_instantiate_sg721_factory_minter() -> (StargazeApp, InitReturn) {
        let (mut app, factory_contract) = valid_instantiate_factory_mint();
        let sg721_id = app.store_code(contract_sg721_base());

        // Set block after SG Genesis
        app.set_block(BlockInfo {
            height: 123456,
            time: Timestamp::from_nanos(1671797419879305533),
            chain_id: "cosmos-testnet-14002".to_string(),
        });

        let mut collection_params = mock_collection_params();
        collection_params.info.creator = ADMIN.to_string();
        let mut m = mock_create_minter(None, collection_params, Some(app.block_info().time));
        m.collection_params.code_id = sg721_id;
        m.init_msg.num_tokens = 10_000u32;
        let msg = ExecuteMsg::CreateMinter(m);

        let creation_fee = coin(CREATION_FEE, NATIVE_DENOM);

        // Distribute some coins
        for wal in [ADMIN, WALLET1, WALLET2].iter() {
            let mint_denom_outcome = app.sudo(SudoMsg::Bank(BankSudo::Mint {
                to_address: wal.to_string(),
                amount: vec![creation_fee.clone()],
            }));
            assert!(mint_denom_outcome.is_ok());
            let minter_bal_denom = app.wrap().query_all_balances(wal.to_string()).unwrap();
            assert_eq!(minter_bal_denom, vec![creation_fee.clone()]);
        }

        let cosmos_msg = factory_contract
            .call_with_funds(msg.clone(), coin(CREATION_FEE, NATIVE_DENOM))
            .unwrap();
        let res = app.execute(Addr::unchecked(ADMIN), cosmos_msg.clone());
        assert!(res.is_ok());

        (
            app,
            InitReturn {
                minter_addr: Addr::unchecked("contract1"),
                nft_addr: Addr::unchecked("contract2"),
            },
        )
    }

    fn validate_execution_outcome(
        tx_outcome: anyhow::Result<AppResponse>,
        error_string_msg: Option<&str>,
    ) {
        if error_string_msg.is_some() {
            let err_str = error_string_msg.unwrap();
            if err_str == "" {
                assert!(tx_outcome.is_err());
            } else {
                assert_eq!(
                    tx_outcome.unwrap_err().source().unwrap().to_string(),
                    err_str
                );
            }
        } else {
            assert!(tx_outcome.is_ok());
        }
    }

    #[test]
    fn integration_tests() {
        // Init the NFT and Minter contracts
        let (mut app, addresses) = valid_instantiate_sg721_factory_minter();

        // Mint some NFTs to 2 wallets
        (1..=1000)
            .map(|nft_mint| {
                let recipient = if nft_mint > 300 { WALLET2 } else { WALLET1 };

                vending_minter::msg::ExecuteMsg::MintFor {
                    token_id: nft_mint,
                    recipient: recipient.to_string(),
                }
            })
            .for_each(|mint_msg| {
                let res = app.execute_contract(
                    Addr::unchecked(ADMIN.to_string()),
                    addresses.minter_addr.clone(),
                    &mint_msg,
                    &[],
                );
                validate_execution_outcome(res, None);
            });

        // Init the NFT reward contract
        let (mut app, nft_reward_dist_addr) =
            valid_instantiate_nft_reward_distribution(app, addresses.nft_addr.clone());

        // Check the Distribution state
        let query_result: crate::state::Distribution = app
            .wrap()
            .query_wasm_smart(
                nft_reward_dist_addr.clone(),
                &crate::msg::QueryMsg::GetCurrentDistributionInfo {},
            )
            .unwrap();
        assert_eq!(query_result.assets.len(), 0);
        assert_eq!(query_result.claimed, 0);

        // Check the Config state
        let query_result: crate::state::Config = app
            .wrap()
            .query_wasm_smart(
                nft_reward_dist_addr.clone(),
                &crate::msg::QueryMsg::GetConfig {},
            )
            .unwrap();
        assert_eq!(query_result.nft_count, 1000);
        assert_eq!(query_result.distributor, Addr::unchecked(DISTRIBUTOR));
        assert_eq!(query_result.current_dist_id, 0);
        assert_eq!(
            query_result.nft_address,
            Addr::unchecked(addresses.nft_addr.clone())
        );
        assert_eq!(query_result.current_dist_end_time, None);

        // Below is used to add distributions end time
        let end_time_distribution = &app.block_info().time.plus_days(1u64);

        // Distribution cannot be added by an unauthorized wallet
        let execute_outcome = app.execute_contract(
            Addr::unchecked(WALLET2.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::Distribute {
                assets_per_nft: vec![
                    Coin {
                        denom: NATIVE_DENOM.to_string(),
                        amount: Uint128::new(100_000_000u128),
                    },
                    Coin {
                        denom: NATIVE_DENOM1.to_string(),
                        amount: Uint128::new(200_000_000u128),
                    },
                    Coin {
                        denom: NATIVE_DENOM2.to_string(),
                        amount: Uint128::new(300_000_000u128),
                    },
                ],
                distribution_end_time: end_time_distribution.clone(),
                unclaimed_to_distributor: false,
                minimum_nfts_to_claim: 1,
            },
            &[coin(50000000u128, NATIVE_DENOM)],
        );
        validate_execution_outcome(execute_outcome, Some("Unauthorized"));

        // Try to claim without an ongoing distribution
        let token_ids: TokensResponse = app
            .wrap()
            .query_wasm_smart(
                addresses.nft_addr.clone(),
                &sg721_base::msg::QueryMsg::Tokens {
                    owner: WALLET2.to_string(),
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap();
        let execute_outcome = app.execute_contract(
            Addr::unchecked(WALLET2.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::BulkClaim {
                token_ids: token_ids.tokens,
                start_after: None,
            },
            &[],
        );
        validate_execution_outcome(execute_outcome, Some(""));

        // Add distribution without funds
        let execute_outcome = app.execute_contract(
            Addr::unchecked(DISTRIBUTOR.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::Distribute {
                assets_per_nft: vec![
                    Coin {
                        denom: NATIVE_DENOM.to_string(),
                        amount: Uint128::new(100_000_000u128),
                    },
                    Coin {
                        denom: NATIVE_DENOM1.to_string(),
                        amount: Uint128::new(200_000_000u128),
                    },
                    Coin {
                        denom: NATIVE_DENOM2.to_string(),
                        amount: Uint128::new(300_000_000u128),
                    },
                ],
                distribution_end_time: end_time_distribution.clone(),
                unclaimed_to_distributor: false,
                minimum_nfts_to_claim: 1,
            },
            &[],
        );
        validate_execution_outcome(execute_outcome, Some("InvalidFundsReceived"));

        // Fund the distribution wallet
        for i in [
            NATIVE_DENOM,
            NATIVE_DENOM1,
            NATIVE_DENOM2,
            NATIVE_DENOM3,
            NATIVE_DENOM4,
        ]
            .iter()
        {
            let mint_denom_outcome = app.sudo(SudoMsg::Bank(BankSudo::Mint {
                to_address: DISTRIBUTOR.to_string(),
                amount: vec![Coin {
                    denom: i.to_string(),
                    amount: Uint128::new(100_000_000_000_000u128),
                }],
            }));
            validate_execution_outcome(mint_denom_outcome, None);
        }

        // Add distribution with invalid funds being sent
        let execute_outcome = app.execute_contract(
            Addr::unchecked(DISTRIBUTOR.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::Distribute {
                assets_per_nft: vec![
                    Coin {
                        denom: NATIVE_DENOM.to_string(),
                        amount: Uint128::new(100_000_000u128),
                    },
                    Coin {
                        denom: NATIVE_DENOM1.to_string(),
                        amount: Uint128::new(200_000_000u128),
                    },
                    Coin {
                        denom: NATIVE_DENOM2.to_string(),
                        amount: Uint128::new(300_000_000u128),
                    },
                ],
                distribution_end_time: end_time_distribution.clone(),
                unclaimed_to_distributor: false,
                minimum_nfts_to_claim: 1,
            },
            &[
                Coin {
                    denom: NATIVE_DENOM3.to_string(),
                    amount: Uint128::new(300_000_000u128 * 1000u128),
                },
                Coin {
                    denom: NATIVE_DENOM.to_string(),
                    amount: Uint128::new(100_000_000u128 * 1000u128),
                },
                Coin {
                    denom: NATIVE_DENOM1.to_string(),
                    amount: Uint128::new(200_000_000u128 * 1000u128),
                },
            ],
        );
        validate_execution_outcome(
            execute_outcome,
            Some("InvalidDistributionInputs"),
        );

        // Add distribution with invalid funds being sent
        let execute_outcome = app.execute_contract(
            Addr::unchecked(DISTRIBUTOR.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::Distribute {
                assets_per_nft: vec![
                    Coin {
                        denom: NATIVE_DENOM.to_string(),
                        amount: Uint128::new(100_000_000u128),
                    },
                    Coin {
                        denom: NATIVE_DENOM1.to_string(),
                        amount: Uint128::new(200_000_000u128),
                    },
                    Coin {
                        denom: NATIVE_DENOM2.to_string(),
                        amount: Uint128::new(300_000_000u128),
                    },
                ],
                distribution_end_time: end_time_distribution.clone(),
                unclaimed_to_distributor: false,
                minimum_nfts_to_claim: 1,
            },
            &[
                Coin {
                    denom: NATIVE_DENOM.to_string(),
                    amount: Uint128::new(100_000_000u128 * 1000u128),
                },
                Coin {
                    denom: NATIVE_DENOM1.to_string(),
                    amount: Uint128::new(200_000_000u128 * 1000u128),
                },
            ],
        );
        validate_execution_outcome(execute_outcome, Some("InvalidFundsReceived"));

        // Add distribution with invalid funds being sent
        let execute_outcome = app.execute_contract(
            Addr::unchecked(DISTRIBUTOR.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::Distribute {
                assets_per_nft: vec![Coin {
                    denom: NATIVE_DENOM2.to_string(),
                    amount: Uint128::new(300_000_000u128),
                }],
                distribution_end_time: end_time_distribution.clone(),
                unclaimed_to_distributor: false,
                minimum_nfts_to_claim: 1,
            },
            &[
                Coin {
                    denom: NATIVE_DENOM.to_string(),
                    amount: Uint128::new(100_000_000u128 * 1000u128),
                },
                Coin {
                    denom: NATIVE_DENOM1.to_string(),
                    amount: Uint128::new(200_000_000u128 * 1000u128),
                },
            ],
        );
        validate_execution_outcome(execute_outcome, Some("InvalidFundsReceived"));

        // Add distribution with invalid funds being sent
        let execute_outcome = app.execute_contract(
            Addr::unchecked(DISTRIBUTOR.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::Distribute {
                assets_per_nft: vec![],
                distribution_end_time: end_time_distribution.clone(),
                unclaimed_to_distributor: false,
                minimum_nfts_to_claim: 1,
            },
            &[
                Coin {
                    denom: NATIVE_DENOM.to_string(),
                    amount: Uint128::new(100_000_000u128 * 1000u128),
                },
                Coin {
                    denom: NATIVE_DENOM1.to_string(),
                    amount: Uint128::new(200_000_000u128 * 1000u128),
                },
            ],
        );
        validate_execution_outcome(execute_outcome, Some("InvalidFundsReceived"));

        // Invalid time end for a new distribution
        let execute_outcome = app.execute_contract(
            Addr::unchecked(DISTRIBUTOR.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::Distribute {
                assets_per_nft: vec![
                    Coin {
                        denom: NATIVE_DENOM.to_string(),
                        amount: Uint128::new(100_000_000u128),
                    },
                    Coin {
                        denom: NATIVE_DENOM1.to_string(),
                        amount: Uint128::new(200_000_000u128),
                    },
                ],
                distribution_end_time: end_time_distribution.clone().minus_days(2u64), // before the actual time now + 1 day
                unclaimed_to_distributor: false,
                minimum_nfts_to_claim: 1,
            },
            &[
                Coin {
                    denom: NATIVE_DENOM.to_string(),
                    amount: Uint128::new(100_000_000u128 * 1000u128),
                },
                Coin {
                    denom: NATIVE_DENOM1.to_string(),
                    amount: Uint128::new(200_000_000u128 * 1000u128),
                },
            ],
        );
        validate_execution_outcome(
            execute_outcome,
            Some("InvalidDistributionEndTime"),
        );

        // We now add a valid distribution
        let execute_outcome = app.execute_contract(
            Addr::unchecked(DISTRIBUTOR.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::Distribute {
                assets_per_nft: vec![
                    Coin {
                        denom: NATIVE_DENOM.to_string(),
                        amount: Uint128::new(100_000_000u128),
                    },
                    Coin {
                        denom: NATIVE_DENOM1.to_string(),
                        amount: Uint128::new(200_000_000u128),
                    },
                ],
                distribution_end_time: end_time_distribution.clone(),
                unclaimed_to_distributor: false,
                minimum_nfts_to_claim: 1,
            },
            &[
                Coin {
                    denom: NATIVE_DENOM.to_string(),
                    amount: Uint128::new(100_000_000u128 * 1000u128),
                },
                Coin {
                    denom: NATIVE_DENOM1.to_string(),
                    amount: Uint128::new(200_000_000u128 * 1000u128),
                },
            ],
        );
        validate_execution_outcome(execute_outcome, None);

        // Check the distribution state
        let query_result: crate::state::Distribution = app
            .wrap()
            .query_wasm_smart(
                nft_reward_dist_addr.clone(),
                &crate::msg::QueryMsg::GetCurrentDistributionInfo {},
            )
            .unwrap();
        assert_eq!(
            query_result,
            crate::state::Distribution {
                assets: vec![
                    Coin {
                        denom: NATIVE_DENOM.to_string(),
                        amount: Uint128::new(100_000_000u128)
                    },
                    Coin {
                        denom: NATIVE_DENOM1.to_string(),
                        amount: Uint128::new(200_000_000u128)
                    }
                ],
                claimed: 0,
                unclaimed_to_distributor: false,
                unclaimed_sent_to_distributor: None,
                minimum_nfts_to_claim: 1
            }
        );

        // Try to halt from a non-admin wallet
        // This can be used if something happens
        let execute_outcome = app.execute_contract(
            Addr::unchecked(DISTRIBUTOR.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::HaltDistribution {},
            &[],
        );
        validate_execution_outcome(execute_outcome, Some("Unauthorized"));

        // Check the Config state after trying to halt and adding a distribution
        let query_result: crate::state::Config = app
            .wrap()
            .query_wasm_smart(
                nft_reward_dist_addr.clone(),
                &crate::msg::QueryMsg::GetConfig {},
            )
            .unwrap();
        assert_eq!(query_result.nft_count, 1000);
        assert_eq!(query_result.current_dist_halted, false);
        assert_eq!(query_result.distributor, Addr::unchecked(DISTRIBUTOR));
        assert_eq!(query_result.current_dist_id, 1);
        assert_eq!(
            query_result.nft_address,
            Addr::unchecked(addresses.nft_addr.clone())
        );
        assert_eq!(query_result.current_dist_end_time, Some(end_time_distribution.clone()));

        // Try to halt from admin
        let execute_outcome = app.execute_contract(
            Addr::unchecked(ADMIN.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::HaltDistribution {},
            &[],
        );
        validate_execution_outcome(execute_outcome, None);

        // Check the Config state after trying to halt and adding a distribution
        let query_result: crate::state::Config = app
            .wrap()
            .query_wasm_smart(
                nft_reward_dist_addr.clone(),
                &crate::msg::QueryMsg::GetConfig {},
            )
            .unwrap();
        assert_eq!(query_result.current_dist_halted, true);

        // Check the balances in the contract (after adding a distribution)
        let reward_balance = app
            .wrap()
            .query_all_balances(nft_reward_dist_addr.to_string())
            .unwrap();
        for coin in reward_balance.iter() {
            if coin.denom == NATIVE_DENOM.to_string() {
                assert!(coin.amount == Uint128::new(100_000_000u128 * 1000u128))
            }
            if coin.denom == NATIVE_DENOM1.to_string() {
                assert!(coin.amount == Uint128::new(200_000_000u128 * 1000u128))
            }
        }

        // Wallet 1 try to claims while halted
        let execute_outcome = app.execute_contract(
            Addr::unchecked(WALLET1.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::BulkClaim {
                token_ids: vec!["1".to_string()],
                start_after: None,
            },
            &[],
        );
        validate_execution_outcome(execute_outcome, Some("DistributionHalted"));

        // Remove halt by admin
        let execute_outcome = app.execute_contract(
            Addr::unchecked(ADMIN.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::HaltDistribution {},
            &[],
        );
        validate_execution_outcome(execute_outcome, None);

        // Wallet 3 tries to claim without NFTs
        let token_ids: TokensResponse = app
            .wrap()
            .query_wasm_smart(
                addresses.nft_addr.clone(),
                &sg721_base::QueryMsg::Tokens {
                    owner: WALLET3.to_string(),
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap();
        let execute_outcome = app.execute_contract(
            Addr::unchecked(WALLET3.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::BulkClaim {
                token_ids: token_ids.tokens,
                start_after: None,
            },
            &[],
        );
        validate_execution_outcome(execute_outcome, Some("NotEnoughNftsToClaim"));

        // Wallet 1 claims
        let before_claim_balance_native_0 = app
            .wrap()
            .query_balance(WALLET1.to_string(), NATIVE_DENOM)
            .unwrap();

        // Check initial balances for Wallet 1 - should be the same
        assert_eq!(
            before_claim_balance_native_0.amount,
            Uint128::new(5_000_000_000u128)
        );
        let before_claim_balance_native_1 = app
            .wrap()
            .query_balance(WALLET1.to_string(), NATIVE_DENOM1)
            .unwrap();
        assert_eq!(before_claim_balance_native_1.amount, Uint128::zero());
        let before_claim_balance_native_2 = app
            .wrap()
            .query_balance(WALLET1.to_string(), NATIVE_DENOM2)
            .unwrap();
        assert_eq!(before_claim_balance_native_2.amount, Uint128::zero());
        let before_claim_balance_native_3 = app
            .wrap()
            .query_balance(WALLET1.to_string(), NATIVE_DENOM3)
            .unwrap();
        assert_eq!(before_claim_balance_native_3.amount, Uint128::zero());

        // Actual Claim of Wallet1
        // Has 1 - 300
        // Claims for 1 - 100
        let execute_outcome = app.execute_contract(
            Addr::unchecked(WALLET1.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::BulkClaim {
                token_ids: (1..=100).map(|num| num.to_string()).collect(),
                start_after: None,
            },
            &[],
        );
        validate_execution_outcome(execute_outcome, None);

        // Check the final balances if it correspond to the expected received amounts
        let after_claim_balance_native_0 = app
            .wrap()
            .query_balance(WALLET1.to_string(), NATIVE_DENOM)
            .unwrap();
        assert_eq!(
            after_claim_balance_native_0.amount,
            Uint128::new(5_000_000_000u128 + 100 * 100_000_000u128)
        );
        let after_claim_balance_native_1 = app
            .wrap()
            .query_balance(WALLET1.to_string(), NATIVE_DENOM1)
            .unwrap();
        assert_eq!(
            after_claim_balance_native_1.amount,
            Uint128::new(100 * 200_000_000u128)
        );
        let after_claim_balance_native_2 = app
            .wrap()
            .query_balance(WALLET1.to_string(), NATIVE_DENOM2)
            .unwrap();
        assert_eq!(after_claim_balance_native_2.amount, Uint128::zero());
        let after_claim_balance_native_3 = app
            .wrap()
            .query_balance(WALLET1.to_string(), NATIVE_DENOM3)
            .unwrap();
        assert_eq!(after_claim_balance_native_3.amount, Uint128::zero());

        // Check balance in the contract -> should decrease by 100 nfts' worth
        let reward_balance = app
            .wrap()
            .query_all_balances(nft_reward_dist_addr.to_string())
            .unwrap();
        for coin in reward_balance.iter() {
            if coin.denom == NATIVE_DENOM.to_string() {
                assert_eq!(coin.amount, Uint128::new(100_000_000u128 * 900u128))
            }
            if coin.denom == NATIVE_DENOM1.to_string() {
                assert_eq!(coin.amount, Uint128::new(200_000_000u128 * 900u128))
            }
        }

        // We then try the non-bulk claim
        // We should be able to claim 101-105
        let execute_outcome = app.execute_contract(
            Addr::unchecked(WALLET1.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::ClaimFive {
                token_ids: (101..=105).map(|num| num.to_string()).collect(),
            },
            &[],
        );
        validate_execution_outcome(execute_outcome, None);

        // Try a bulk of the same - should not work
        let execute_outcome = app.execute_contract(
            Addr::unchecked(WALLET1.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::BulkClaim {
                token_ids: (101..=105).map(|num| num.to_string()).collect(),
                start_after: None,
            },
            &[],
        );
        validate_execution_outcome(execute_outcome, Some("NothingToClaim"));

        // Try a 5 of the same - should not work
        let execute_outcome = app.execute_contract(
            Addr::unchecked(WALLET1.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::ClaimFive {
                token_ids: (55..=59).map(|num| num.to_string()).collect(),
            },
            &[],
        );
        validate_execution_outcome(execute_outcome, Some("NothingToClaim"));

        // Try a 5 but by sending more than 5
        let execute_outcome = app.execute_contract(
            Addr::unchecked(WALLET1.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::ClaimFive {
                token_ids: (110..=120).map(|num| num.to_string()).collect(),
            },
            &[],
        );
        validate_execution_outcome(execute_outcome, Some("TooManyTokensSent"));

        // Try to claim 700 NFTs from bulk
        let execute_outcome = app.execute_contract(
            Addr::unchecked(WALLET2.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::BulkClaim {
                token_ids: (301..=1000).map(|num| num.to_string()).collect(),
                start_after: None,
            },
            &[],
        );
        validate_execution_outcome(execute_outcome, Some("TooManyTokensSent"));

        // Adjust time block time and try to claim after the window
        app.set_block(BlockInfo {
            height: 123456,
            time: end_time_distribution.plus_days(1u64),
            chain_id: "cosmos-testnet-14002".to_string(),
        });

        // Wallet 1 never claimed
        let execute_outcome = app.execute_contract(
            Addr::unchecked(WALLET1.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::BulkClaim {
                token_ids: vec!["TOKEN".to_string()],
                start_after: None,
            },
            &[],
        );
        validate_execution_outcome(execute_outcome, Some("ClaimingWindowHasClosed"));

        // Confirm that 1-105 are claimed
        for tok_id in (1..=105)
            .map(|num| num.to_string())
            .collect::<Vec<String>>()
            .iter()
        {
            let claimed: bool = app
                .wrap()
                .query_wasm_smart(
                    nft_reward_dist_addr.to_string(),
                    &crate::msg::QueryMsg::GetIfClaimed {
                        token_id: tok_id.to_string(),
                    },
                )
                .unwrap();
            assert!(claimed);
        }
        // Confirm that the rest isnt
        for tok_id in (106..=1000)
            .map(|num| num.to_string())
            .collect::<Vec<String>>()
            .iter()
        {
            let claimed: bool = app
                .wrap()
                .query_wasm_smart(
                    nft_reward_dist_addr.to_string(),
                    &crate::msg::QueryMsg::GetIfClaimed {
                        token_id: tok_id.to_string(),
                    },
                )
                .unwrap();
            assert!(!claimed);
        }

        // If a new distribution happens, the remaining unclaimed from the previous dist
        // should be split to the holders in the new distribution
        let execute_outcome = app.execute_contract(
            Addr::unchecked(DISTRIBUTOR.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::Distribute {
                assets_per_nft: vec![
                    Coin {
                        denom: NATIVE_DENOM.to_string(),
                        amount: Uint128::new(100_000_000u128),
                    },
                    Coin {
                        denom: NATIVE_DENOM2.to_string(),
                        amount: Uint128::new(200_000_000u128),
                    },
                    Coin {
                        denom: NATIVE_DENOM3.to_string(),
                        amount: Uint128::new(300_000_000u128),
                    },
                ],
                distribution_end_time: end_time_distribution.plus_days(2u64).clone(),
                unclaimed_to_distributor: false,
                minimum_nfts_to_claim: 1,
            },
            &[
                Coin {
                    denom: NATIVE_DENOM.to_string(),
                    amount: Uint128::new(100_000_000u128 * 1000u128),
                },
                Coin {
                    denom: NATIVE_DENOM2.to_string(),
                    amount: Uint128::new(200_000_000u128 * 1000u128),
                },
                Coin {
                    denom: NATIVE_DENOM3.to_string(),
                    amount: Uint128::new(300_000_000u128 * 1000u128),
                },
            ],
        );
        validate_execution_outcome(execute_outcome, None);

        // Now we make Wallet 1 and 2 claim everything
        // Claim
        // The below will claim 301-799 given 1000 is presented before but will be discarded
        // So 800 will be put as "not found"
        let execute_outcome = app.execute_contract(
            Addr::unchecked(WALLET2.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::BulkClaim {
                token_ids: (301..=800).map(|num| num.to_string()).collect(),
                start_after: None,
            },
            &[],
        );
        assert_eq!(
            execute_outcome
                .unwrap()
                .events
                .iter()
                .find(|event| event.ty == "wasm".to_string())
                .unwrap()
                .attributes
                .iter()
                .find(|attr| attr.key == "Tokens not found or not owned: ".to_string())
                .unwrap()
                .value,
            "800".to_string()
        );
        // As per the contract, there is a max 5 queries for the bulk. So there needs to be a start_after to be sent in the case the person
        // has more than 500 NFTs
        // The below should only pay for 1000 given the ordering in the state
        let execute_outcome = app.execute_contract(
            Addr::unchecked(WALLET2.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::BulkClaim {
                token_ids: (801..=1000).map(|num| num.to_string()).collect(),
                start_after: None,
            },
            &[],
        );
        assert_eq!(
            execute_outcome
                .unwrap()
                .events
                .iter()
                .find(|event| event.ty == "wasm".to_string())
                .unwrap()
                .attributes
                .iter()
                .find(|attr| attr.key == "Tokens not found or not owned: ".to_string())
                .unwrap()
                .value,
            (801..=999)
                .map(|num| num.to_string())
                .collect::<Vec<String>>()
                .join(", ")
        );
        let execute_outcome = app.execute_contract(
            Addr::unchecked(WALLET2.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::ClaimFive {
                token_ids: vec!["1000".to_string()],
            },
            &[],
        );
        validate_execution_outcome(execute_outcome, Some("NothingToClaim"));

        // Claim with a start after to catch 800-999
        let execute_outcome = app.execute_contract(
            Addr::unchecked(WALLET2.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::BulkClaim {
                token_ids: (800..=1000).map(|num| num.to_string()).collect(),
                start_after: Some("799".to_string()),
            },
            &[],
        );
        // 1000 should already be claimed
        assert_eq!(
            execute_outcome
                .unwrap()
                .events
                .iter()
                .find(|event| event.ty == "wasm".to_string())
                .unwrap()
                .attributes
                .iter()
                .find(|attr| attr.key == "Tokens not found or not owned: ".to_string())
                .unwrap()
                .value,
            "1000".to_string()
        );

        // Claim 300 from wallet 1
        let execute_outcome = app.execute_contract(
            Addr::unchecked(WALLET1.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::BulkClaim {
                token_ids: (1..=300).map(|num| num.to_string()).collect(),
                start_after: None,
            },
            &[],
        );
        validate_execution_outcome(execute_outcome, None);

        // Everything should be claimed
        let query_result: crate::state::Distribution = app
            .wrap()
            .query_wasm_smart(
                nft_reward_dist_addr.clone(),
                &crate::msg::QueryMsg::GetCurrentDistributionInfo {},
            )
            .unwrap();
        assert_eq!(query_result.claimed, 1000);

        let after_claim_balance_native_0 = app
            .wrap()
            .query_balance(WALLET2.to_string(), NATIVE_DENOM)
            .unwrap();
        assert_eq!(
            after_claim_balance_native_0.amount,
            Uint128::new(
                CREATION_FEE
                    + (700 * 100_000_000u128) // current + unclaimed
                    + ((895 * 100_000_000u128) / 1000) * 700 // unclaimed portion
            )
        );
        let after_claim_balance_native_1 = app
            .wrap()
            .query_balance(WALLET2.to_string(), NATIVE_DENOM1)
            .unwrap();
        assert_eq!(
            after_claim_balance_native_1.amount,
            Uint128::new(
                ((895 * 200_000_000u128) / 1000) * 700 // unclaimed portion
            )
        );
        let after_claim_balance_native_2 = app
            .wrap()
            .query_balance(WALLET2.to_string(), NATIVE_DENOM2)
            .unwrap();
        assert_eq!(
            after_claim_balance_native_2.amount,
            Uint128::new(700u128 * 200_000_000u128)
        );
        let after_claim_balance_native_3 = app
            .wrap()
            .query_balance(WALLET2.to_string(), NATIVE_DENOM3)
            .unwrap();
        assert_eq!(
            after_claim_balance_native_3.amount,
            Uint128::new(700u128 * 300_000_000u128)
        );

        // Should be nothing left in the contract
        let reward_bals = app
            .wrap()
            .query_all_balances(nft_reward_dist_addr.to_string())
            .unwrap();
        assert!(reward_bals.len() == 0);
        for bal in reward_bals.iter() {
            assert!(bal.amount == Uint128::zero());
        }

        // Try to distribute before the end of the current distribution
        let execute_outcome = app.execute_contract(
            Addr::unchecked(DISTRIBUTOR.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::Distribute {
                assets_per_nft: vec![
                    Coin {
                        denom: NATIVE_DENOM.to_string(),
                        amount: Uint128::new(100_000_000u128),
                    },
                    Coin {
                        denom: NATIVE_DENOM2.to_string(),
                        amount: Uint128::new(200_000_000u128),
                    },
                    Coin {
                        denom: NATIVE_DENOM3.to_string(),
                        amount: Uint128::new(300_000_000u128),
                    },
                ],
                distribution_end_time: end_time_distribution.plus_days(2u64).clone(),
                unclaimed_to_distributor: false,
                minimum_nfts_to_claim: 1,
            },
            &[
                Coin {
                    denom: NATIVE_DENOM.to_string(),
                    amount: Uint128::new(100_000_000u128 * 6u128),
                },
                Coin {
                    denom: NATIVE_DENOM2.to_string(),
                    amount: Uint128::new(200_000_000u128 * 6u128),
                },
                Coin {
                    denom: NATIVE_DENOM3.to_string(),
                    amount: Uint128::new(300_000_000u128 * 6u128),
                },
            ],
        );
        validate_execution_outcome(
            execute_outcome,
            Some("CurrentDistributionHasNotYetEnded"),
        );

        // Reclaim should error
        let execute_outcome = app.execute_contract(
            Addr::unchecked(WALLET1.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::BulkClaim {
                token_ids: vec!["TOKEN".to_string()],
                start_after: None,
            },
            &[],
        );
        validate_execution_outcome(execute_outcome, Some("NothingToClaim"));

        // Check the config state
        let query_result: crate::state::Config = app
            .wrap()
            .query_wasm_smart(
                nft_reward_dist_addr.clone(),
                &crate::msg::QueryMsg::GetConfig {},
            )
            .unwrap();
        assert_eq!(query_result.nft_count, 1000); // 3 for 2 wallets
        assert_eq!(query_result.distributor, Addr::unchecked(DISTRIBUTOR));
        assert_eq!(query_result.current_dist_id, 2);
        assert_eq!(
            query_result.nft_address,
            Addr::unchecked(addresses.nft_addr.clone())
        );
        assert_eq!(
            query_result.current_dist_end_time,
            Some(end_time_distribution.plus_days(2u64).clone())
        );

        // Should all be claimed
        for tok_id in 1..=1000 {
            let claimed: bool = app
                .wrap()
                .query_wasm_smart(
                    nft_reward_dist_addr.to_string(),
                    &crate::msg::QueryMsg::GetIfClaimed {
                        token_id: tok_id.to_string(),
                    },
                )
                .unwrap();
            assert!(claimed);
        }

        // Check if dist query is ok
        let query_result: crate::state::Distribution = app
            .wrap()
            .query_wasm_smart(
                nft_reward_dist_addr.clone(),
                &crate::msg::QueryMsg::GetCurrentDistributionInfo {},
            )
            .unwrap();
        for asset in query_result.assets.iter() {
            if asset.denom == NATIVE_DENOM {
                assert_eq!(
                    asset.amount,
                    Uint128::new(100_000_000u128 + (895 * 100_000_000u128 / 1000u128))
                );
            }
            if asset.denom == NATIVE_DENOM1 {
                assert_eq!(
                    asset.amount,
                    Uint128::new(895 * 200_000_000u128 / 1000u128)
                );
            }
            if asset.denom == NATIVE_DENOM2 {
                assert_eq!(asset.amount, Uint128::new(200_000_000u128));
            }
            if asset.denom == NATIVE_DENOM3 {
                assert_eq!(asset.amount, Uint128::new(300_000_000u128));
            }
        }
        assert_eq!(query_result.claimed, 1000);

        // Now make a dist of unseen coin and make it claimable by the dist if unclaimed
        app.set_block(BlockInfo {
            height: 123456,
            time: end_time_distribution.clone().plus_days(4u64),
            chain_id: "cosmos-testnet-14002".to_string(),
        });
        let execute_outcome = app.execute_contract(
            Addr::unchecked(DISTRIBUTOR.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::Distribute {
                assets_per_nft: vec![Coin {
                    denom: NATIVE_DENOM4.to_string(),
                    amount: Uint128::new(100_000_000u128),
                }],
                distribution_end_time: end_time_distribution.plus_days(5u64).clone(),
                unclaimed_to_distributor: true,
                minimum_nfts_to_claim: 1,
            },
            &[Coin {
                denom: NATIVE_DENOM4.to_string(),
                amount: Uint128::new(100_000_000u128 * 1000u128),
            }],
        );
        validate_execution_outcome(execute_outcome, None);

        // Check if dist query is ok
        let query_result: crate::state::Distribution = app
            .wrap()
            .query_wasm_smart(
                nft_reward_dist_addr.clone(),
                &crate::msg::QueryMsg::GetCurrentDistributionInfo {},
            )
            .unwrap();
        for asset in query_result.assets.iter() {
            if asset.denom == NATIVE_DENOM {
                assert_eq!(asset.amount, Uint128::zero());
            }
            if asset.denom == NATIVE_DENOM1 {
                assert_eq!(asset.amount, Uint128::zero());
            }
            if asset.denom == NATIVE_DENOM2 {
                assert_eq!(asset.amount, Uint128::zero());
            }
            if asset.denom == NATIVE_DENOM3 {
                assert_eq!(asset.amount, Uint128::zero());
            }
            if asset.denom == NATIVE_DENOM4 {
                assert_eq!(asset.amount, Uint128::new(100_000_000u128));
            }
        }
        assert_eq!(query_result.claimed, 0);

        // 100 will be claimed so 900 unclaimed and should be given back to the distributor
        let before_claim_contract_balance_4 = app
            .wrap()
            .query_balance(nft_reward_dist_addr.to_string(), NATIVE_DENOM4)
            .unwrap();
        let _before_claim_balance_distributor = app
            .wrap()
            .query_balance(DISTRIBUTOR.to_string(), NATIVE_DENOM4)
            .unwrap();
        let execute_outcome = app.execute_contract(
            Addr::unchecked(WALLET1.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::BulkClaim {
                token_ids: (1..=100).map(|num| num.to_string()).collect(),
                start_after: None,
            },
            &[],
        );
        validate_execution_outcome(execute_outcome, None);

        // Go after the end period for the dist
        app.set_block(BlockInfo {
            height: 123456,
            time: end_time_distribution.clone().plus_days(10u64),
            chain_id: "cosmos-testnet-14002".to_string(),
        });
        // How much is in the contract after the claim
        let after_claim_contract_balance_4 = app
            .wrap()
            .query_balance(nft_reward_dist_addr.to_string(), NATIVE_DENOM4)
            .unwrap();
        assert_eq!(
            before_claim_contract_balance_4.amount - after_claim_contract_balance_4.amount,
            Uint128::new(100u128 * 100_000_000u128)
        );
        let after_claim_dist_balance_4 = app
            .wrap()
            .query_balance(DISTRIBUTOR.to_string(), NATIVE_DENOM4)
            .unwrap();

        // A non distributor tries to claim
        let execute_outcome = app.execute_contract(
            Addr::unchecked(WALLET1.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::ReturnUnclaimed {},
            &[],
        );
        validate_execution_outcome(execute_outcome, Some("Unauthorized"));

        // Valid claim
        let execute_outcome = app.execute_contract(
            Addr::unchecked(DISTRIBUTOR.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::ReturnUnclaimed {},
            &[],
        );
        validate_execution_outcome(execute_outcome, None);
        let after_returned_contract_balance_4 = app
            .wrap()
            .query_balance(nft_reward_dist_addr.to_string(), NATIVE_DENOM4)
            .unwrap();
        assert_eq!(after_returned_contract_balance_4.amount, Uint128::zero());
        assert_eq!(
            after_claim_contract_balance_4.amount - after_returned_contract_balance_4.amount,
            Uint128::new(900u128 * 100_000_000u128)
        );
        let after_returned_dist_balance_4 = app
            .wrap()
            .query_balance(DISTRIBUTOR.to_string(), NATIVE_DENOM4)
            .unwrap();
        assert_eq!(
            after_returned_dist_balance_4.amount - after_claim_dist_balance_4.amount,
            Uint128::new(900u128 * 100_000_000u128)
        );

        // Should not be able to reclaim
        let execute_outcome = app.execute_contract(
            Addr::unchecked(DISTRIBUTOR.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::ReturnUnclaimed {},
            &[],
        );
        validate_execution_outcome(
            execute_outcome,
            Some("UnclaimedWasAlreadyReturned"),
        );

        // If a new distribution is added, should have no additional assets to distribute
        let execute_outcome = app.execute_contract(
            Addr::unchecked(DISTRIBUTOR.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::Distribute {
                assets_per_nft: vec![
                    Coin {
                        denom: NATIVE_DENOM.to_string(),
                        amount: Uint128::new(100_000_000u128),
                    },
                    Coin {
                        denom: NATIVE_DENOM1.to_string(),
                        amount: Uint128::new(200_000_000u128),
                    },
                    Coin {
                        denom: NATIVE_DENOM2.to_string(),
                        amount: Uint128::new(300_000_000u128),
                    },
                ],
                distribution_end_time: end_time_distribution.plus_days(15u64).clone(),
                unclaimed_to_distributor: true,
                minimum_nfts_to_claim: 1,
            },
            &[
                Coin {
                    denom: NATIVE_DENOM.to_string(),
                    amount: Uint128::new(100_000_000u128 * 1000u128),
                },
                Coin {
                    denom: NATIVE_DENOM1.to_string(),
                    amount: Uint128::new(200_000_000u128 * 1000u128),
                },
                Coin {
                    denom: NATIVE_DENOM2.to_string(),
                    amount: Uint128::new(300_000_000u128 * 1000u128),
                },
            ],
        );
        validate_execution_outcome(execute_outcome, None);

        // THe new distribution should have nothing from the last one
        let query_result: crate::state::Distribution = app
            .wrap()
            .query_wasm_smart(
                nft_reward_dist_addr.clone(),
                &crate::msg::QueryMsg::GetCurrentDistributionInfo {},
            )
            .unwrap();
        assert_eq!(
            query_result.assets[0],
            Coin {
                denom: NATIVE_DENOM.to_string(),
                amount: Uint128::new(100_000_000u128)
            }
        );
        assert_eq!(
            query_result.assets[1],
            Coin {
                denom: NATIVE_DENOM1.to_string(),
                amount: Uint128::new(200_000_000u128)
            }
        );
        assert_eq!(
            query_result.assets[2],
            Coin {
                denom: NATIVE_DENOM2.to_string(),
                amount: Uint128::new(300_000_000u128)
            }
        );
        assert_eq!(query_result.claimed, 0);
        assert_eq!(query_result.unclaimed_to_distributor, true);
        assert_eq!(query_result.unclaimed_sent_to_distributor, Some(false));

        // Also possible that the unclaimed is sent when adding a new dist without doing a return unclaimed
        app.set_block(BlockInfo {
            height: 123456,
            time: end_time_distribution.plus_days(20u64),
            chain_id: "cosmos-testnet-14002".to_string(),
        });
        let before_new_dist = app
            .wrap()
            .query_balance(DISTRIBUTOR.to_string(), NATIVE_DENOM2)
            .unwrap();
        let execute_outcome = app.execute_contract(
            Addr::unchecked(DISTRIBUTOR.to_string()),
            nft_reward_dist_addr.clone(),
            &crate::msg::ExecuteMsg::Distribute {
                assets_per_nft: vec![
                    Coin {
                        denom: NATIVE_DENOM.to_string(),
                        amount: Uint128::new(100_000_000u128),
                    },
                    Coin {
                        denom: NATIVE_DENOM1.to_string(),
                        amount: Uint128::new(200_000_000u128),
                    },
                    Coin {
                        denom: NATIVE_DENOM2.to_string(),
                        amount: Uint128::new(300_000_000u128),
                    },
                ],
                distribution_end_time: end_time_distribution.plus_days(21u64).clone(),
                unclaimed_to_distributor: true,
                minimum_nfts_to_claim: 1,
            },
            &[
                Coin {
                    denom: NATIVE_DENOM.to_string(),
                    amount: Uint128::new(100_000_000u128 * 1000u128),
                },
                Coin {
                    denom: NATIVE_DENOM1.to_string(),
                    amount: Uint128::new(200_000_000u128 * 1000u128),
                },
                Coin {
                    denom: NATIVE_DENOM2.to_string(),
                    amount: Uint128::new(300_000_000u128 * 1000u128),
                },
            ],
        );
        validate_execution_outcome(execute_outcome, None);

        // Distributor should receive the unclaimed amount when the new dist is added
        let after_new_dist = app
            .wrap()
            .query_balance(DISTRIBUTOR.to_string(), NATIVE_DENOM2)
            .unwrap();
        // Different should be 0 -> add new dist so send the assets but receive the
        // old one which was the same amounts
        assert_eq!(
            after_new_dist.amount - before_new_dist.amount,
            Uint128::zero()
        );

        // THe new distribution should have nothing from the last one
        let query_result: crate::state::Distribution = app
            .wrap()
            .query_wasm_smart(
                nft_reward_dist_addr.clone(),
                &crate::msg::QueryMsg::GetCurrentDistributionInfo {},
            )
            .unwrap();
        assert_eq!(
            query_result.assets[0],
            Coin {
                denom: NATIVE_DENOM.to_string(),
                amount: Uint128::new(100_000_000u128)
            }
        );
        assert_eq!(
            query_result.assets[1],
            Coin {
                denom: NATIVE_DENOM1.to_string(),
                amount: Uint128::new(200_000_000u128)
            }
        );
        assert_eq!(
            query_result.assets[2],
            Coin {
                denom: NATIVE_DENOM2.to_string(),
                amount: Uint128::new(300_000_000u128)
            }
        );
        assert_eq!(query_result.claimed, 0);
        assert_eq!(query_result.unclaimed_to_distributor, true);
        assert_eq!(query_result.unclaimed_sent_to_distributor, Some(false));
    }
}
