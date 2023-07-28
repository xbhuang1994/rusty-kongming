// use std::collections::BTreeMap;

// use ethers::prelude::*;
// use revm::primitives::{ExecutionResult, Output, TransactTo, B160 as rAddress, U256 as rU256};

// use crate::prelude::access_list::AccessListInspector;
// use crate::prelude::fork_db::ForkDB;
// use crate::prelude::fork_factory::ForkFactory;
// use crate::prelude::is_sando_safu::{IsSandoSafu, SalmonellaInspectoooor};
// use crate::prelude::sandwich_types::RawIngredients;
// use crate::prelude::{
//     convert_access_list, get_amount_out_evm, get_balance_of_evm, Pool, PoolVariant,
// };
// use crate::types::sandwich_types::OptimalRecipe;
// use crate::types::{BlockInfo, SimulationError};
// use crate::utils::tx_builder::{self, braindance, SandwichMaker};
// use crate::utils::{constants, dotenv};

// use super::{
//     attach_braindance_module, braindance_address, braindance_controller_address,
//     braindance_starting_balance, setup_block_state,
// };

// #[cfg(test)]
// mod test {
//     use std::str::FromStr;

//     use crate::{
//         prelude::{fork_factory::ForkFactory, sandwich_types::RawIngredients},
//         utils::{self, constants, testhelper, tx_builder::SandwichMaker},
//     };
//     use dotenv::dotenv;
//     use ethers::prelude::*;
//     use tokio::{runtime::Runtime, time::Instant};

//     async fn create_test_reverse(
//         fork_block_num: u64,
//         pool_addr: &str,
//         meats: Vec<&str>,
//         is_v2: bool,
//     ) {
//         //TODO
//         panic!("not implemented yet");
//     }
//     async fn create_test(fork_block_num: u64, pool_addr: &str, meats: Vec<&str>, is_v2: bool) {
//         dotenv().ok();
//         // let ws_provider = testhelper::create_ws().await;
//         let ws_provider = utils::create_websocket_client().await.unwrap();

//         let start = Instant::now();

//         let pool = match is_v2 {
//             true => {
//                 testhelper::create_v2_pool(pool_addr.parse::<Address>().unwrap(), &ws_provider)
//                     .await
//             }
//             false => {
//                 testhelper::create_v3_pool(pool_addr.parse::<Address>().unwrap(), &ws_provider)
//                     .await
//             }
//         };

//         let mut victim_txs = vec![];

//         for tx_hash in meats {
//             let tx_hash = TxHash::from_str(tx_hash).unwrap();
//             victim_txs.push(ws_provider.get_transaction(tx_hash).await.unwrap().unwrap());
//         }

//         let state = utils::state_diff::get_from_txs(
//             &ws_provider,
//             &victim_txs,
//             BlockNumber::Number(U64::from(fork_block_num)),
//         )
//         .await
//         .unwrap();

//         let initial_db = utils::state_diff::to_cache_db(
//             &state,
//             Some(BlockId::Number(BlockNumber::Number(fork_block_num.into()))),
//             &ws_provider,
//         )
//         .await
//         .unwrap();
//         let mut db = ForkFactory::new_sandbox_factory(
//             ws_provider.clone(),
//             initial_db,
//             Some(fork_block_num.into()),
//         );

//         let ingredients =
//             RawIngredients::new(&pool, victim_txs, constants::get_weth_address(), state)
//                 .await
//                 .unwrap();

//         match super::create_optimal_sandwich(
//             &ingredients,
//             ethers::utils::parse_ether("50").unwrap(),
//             &testhelper::get_next_block_info(fork_block_num, &ws_provider).await,
//             &mut db,
//             &SandwichMaker::new().await,
//         )
//         .await
//         {
//             Ok(sandwich) => println!("revenue: {:?}", sandwich.revenue),
//             Err(_) => println!("not sandwichable"),
//         };
//         println!("total_duration took: {:?}", start.elapsed());
//     }
//     #[test]
//     fn sandv2_uniswap_router_reverse() {
//         let rt = Runtime::new().unwrap();
//         rt.block_on(async {
//             create_test_reverse(
//                 17760866,
//                 "0x33af110e648c5b8acb21e93ed2fab7d361309014",
//                 vec!["0x4e4309001648bc1660bdb86218af4c1428662ed7fd27d1fc8ec6732afb40c6bb"],
//                 true,
//             )
//             .await;

//             create_test_reverse(
//                 17773300,
//                 "0x60e5d1afef2d253366e87d8298090b7d0ea5d827",
//                 vec!["0xfa792db28c3c56842155a188a69717f79cdad828ab1d0d8b1adea53e6e5ab84a"],
//                 true,
//             )
//             .await;
//         });
//     }
//     #[test]
//     fn sandv2_sushi_router() {
//         // Can't use [tokio::test] attr with `global_backed` for some reason
//         // so manually create a runtime
//         let rt = Runtime::new().unwrap();
//         rt.block_on(async {
//             create_test(
//                 17721757,
//                 "0x7fdeb46b3a0916630f36e886d675602b1007fcbb",
//                 vec!["0xbf5aaafe4d8e1eba3dfb1d05e9b45e9532bd8cb58e1e00a0679041e4bee6c1d0"],
//                 true,
//             )
//             .await;
//         });
//     }

//     #[test]
//     fn sandv3_uniswap_universal_router_one() {
//         // Can't use [tokio::test] attr with `global_backed` for some reason
//         // so manually create a runtime
//         let rt = Runtime::new().unwrap();
//         rt.block_on(async {
//             create_test(
//                 16863224,
//                 "0x62CBac19051b130746Ec4CF96113aF5618F3A212",
//                 vec!["0x90dfe56814821e7f76f2e4970a7b35948670a968abffebb7be69fe528283e6d8"],
//                 false,
//             )
//             .await;
//         });
//     }

//     #[test]
//     fn sandv3_uniswap_universal_router_two() {
//         // Can't use [tokio::test] attr with `global_backed` for some reason
//         // so manually create a runtime
//         let rt = Runtime::new().unwrap();
//         rt.block_on(async {
//             create_test(
//                 16863008,
//                 "0xa80838D2BB3d6eBaEd1978FA23b38F91775D8378",
//                 vec!["0xcb0d4dc905ae0662e5f18b4ad0c2af4e700e8b5969d878a2dcfd0d9507435f4d"],
//                 false,
//             )
//             .await;
//         });
//     }

//     #[test]
//     fn sandv2_kyber_swap() {
//         // Can't use [tokio::test] attr with `global_backed` for some reason
//         // so manually create a runtime
//         let rt = Runtime::new().unwrap();
//         rt.block_on(async {
//             create_test(
//                 16863312,
//                 "0x08650bb9dc722C9c8C62E79C2BAfA2d3fc5B3293",
//                 vec!["0x907894174999fdddc8d8f8e90c210cdb894b91c2c0d79ac35603007d3ce54d00"],
//                 true,
//             )
//             .await;
//         });
//     }

//     #[test]
//     fn sandv2_non_sandwichable() {
//         // Can't use [tokio::test] attr with `global_backed` for some reason
//         // so manually create a runtime
//         let rt = Runtime::new().unwrap();
//         rt.block_on(async {
//             create_test(
//                 16780624,
//                 "0x657c6a08d49b4f0778f9cce1dc49d196cfce9d08",
//                 vec!["0x77b0b15a3216885a66b3b800173e0edcae9d8d191f7093b99a46fc9346f67466"],
//                 true,
//             )
//             .await;
//         });
//     }

//     #[test]
//     fn sandv2_multi_with_three_expect_one_reverts() {
//         // Can't use [tokio::test] attr with `global_backed` for some reason
//         // so manually create a runtime
//         let rt = Runtime::new().unwrap();
//         rt.block_on(async {
//             create_test(
//                 16780624,
//                 "0x657c6a08d49b4f0778f9cce1dc49d196cfce9d08",
//                 vec![
//                     "0x4791d05bdd6765f036ff4ae44fc27099997417e3bdb053ecb52182bbfc7767c5",
//                     "0x923c9ba97fea8d72e60c14d1cc360a8e7d99dd4b31274928d6a79704a8546eda",
//                     "0x77b0b15a3216885a66b3b800173e0edcae9d8d191f7093b99a46fc9346f67466",
//                 ],
//                 true,
//             )
//             .await;
//         });
//     }

//     #[test]
//     fn sandv2_multi_two() {
//         // Can't use [tokio::test] attr with `global_backed` for some reason
//         // so manually create a runtime
//         let rt = Runtime::new().unwrap();
//         rt.block_on(async {
//             create_test(
//                 16780624,
//                 "0x657c6a08d49B4F0778f9cce1Dc49d196cFCe9d08",
//                 vec![
//                     "0x4791d05bdd6765f036ff4ae44fc27099997417e3bdb053ecb52182bbfc7767c5",
//                     "0x923c9ba97fea8d72e60c14d1cc360a8e7d99dd4b31274928d6a79704a8546eda",
//                 ],
//                 true,
//             )
//             .await;
//         });
//     }

//     #[test]
//     fn sandv2_metamask_swap_router() {
//         // Can't use [tokio::test] attr with `global_backed` for some reason
//         // so manually create a runtime
//         let rt = Runtime::new().unwrap();
//         rt.block_on(async {
//             create_test(
//                 16873743,
//                 "0x7A9dDcf06260404D14AbE3bE99c1804D2A5239ce",
//                 vec!["0xcce01725bf7abfab3a4a533275cb4558a66d7794153b4ec01debaf5abd0dc21f"],
//                 true,
//             )
//             .await;
//         });
//     }
// }
