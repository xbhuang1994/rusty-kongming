#[cfg(test)]
mod test {
    use std::{str::FromStr, vec};

    use crate::{
        prelude::{fork_factory::ForkFactory, sandwich_types::RawIngredients},
        utils::{self, constants, testhelper, tx_builder::SandwichMaker},
    };
    use dotenv::dotenv;
    use ethers::prelude::*;
    use tokio::{runtime::Runtime, time::Instant};

    use crate::simulate::sandwich::{cook_simple_forward, cook_simple_reverse};

    async fn create_test_reverse(
        fork_block_num: u64,
        pool_addr: &str,
        meats: Vec<&str>,
        is_v2: bool,
        test_flag: &str,
    ) {
        dotenv().ok();
        // let ws_provider = testhelper::create_ws().await;
        let ws_provider = utils::create_websocket_client().await.unwrap();

        let start = Instant::now();

        let pool = match is_v2 {
            true => {
                testhelper::create_v2_pool(pool_addr.parse::<Address>().unwrap(), &ws_provider)
                    .await
            }
            false => {
                testhelper::create_v3_pool(pool_addr.parse::<Address>().unwrap(), &ws_provider)
                    .await
            }
        };

        let mut victim_txs = vec![];

        for tx_hash in meats {
            let tx_hash = TxHash::from_str(tx_hash).unwrap();
            victim_txs.push(ws_provider.get_transaction(tx_hash).await.unwrap().unwrap());
        }

        let state = utils::state_diff::get_from_txs(
            &ws_provider,
            &victim_txs,
            BlockNumber::Number(U64::from(fork_block_num)),
        )
        .await
        .unwrap();

        let initial_db = utils::state_diff::to_cache_db(
            &state,
            Some(BlockId::Number(BlockNumber::Number(fork_block_num.into()))),
            &ws_provider,
        )
        .await
        .unwrap();
        let mut db = ForkFactory::new_sandbox_factory(
            ws_provider.clone(),
            initial_db,
            Some(fork_block_num.into()),
        );

        let ingredients =
            RawIngredients::new(&pool, victim_txs, constants::get_weth_address(), state)
                .await
                .unwrap();

        match cook_simple_reverse::create_optimal_sandwich(
            &ingredients,
            ethers::utils::parse_ether("50").unwrap(),
            &testhelper::get_next_block_info(fork_block_num, &ws_provider).await,
            &mut db,
            &SandwichMaker::new().await,
        )
        .await
        {
            Ok(sandwich) => println!("{} revenue: {:?}", test_flag, sandwich.revenue),
            Err(_) => println!("{} not sandwichable", test_flag),
        };
        println!("{} total_duration took: {:?}", test_flag, start.elapsed());
    }
    async fn create_test(fork_block_num: u64, pool_addr: &str, meats: Vec<&str>, is_v2: bool) {
        dotenv().ok();
        // let ws_provider = testhelper::create_ws().await;
        let ws_provider = utils::create_websocket_client().await.unwrap();

        let start = Instant::now();

        let pool = match is_v2 {
            true => {
                testhelper::create_v2_pool(pool_addr.parse::<Address>().unwrap(), &ws_provider)
                    .await
            }
            false => {
                testhelper::create_v3_pool(pool_addr.parse::<Address>().unwrap(), &ws_provider)
                    .await
            }
        };

        let mut victim_txs = vec![];

        for tx_hash in meats {
            let tx_hash = TxHash::from_str(tx_hash).unwrap();
            victim_txs.push(ws_provider.get_transaction(tx_hash).await.unwrap().unwrap());
        }

        let state = utils::state_diff::get_from_txs(
            &ws_provider,
            &victim_txs,
            BlockNumber::Number(U64::from(fork_block_num)),
        )
        .await
        .unwrap();

        let initial_db = utils::state_diff::to_cache_db(
            &state,
            Some(BlockId::Number(BlockNumber::Number(fork_block_num.into()))),
            &ws_provider,
        )
        .await
        .unwrap();
        let mut db = ForkFactory::new_sandbox_factory(
            ws_provider.clone(),
            initial_db,
            Some(fork_block_num.into()),
        );

        let ingredients =
            RawIngredients::new(&pool, victim_txs, constants::get_weth_address(), state)
                .await
                .unwrap();

        match cook_simple_forward::create_optimal_sandwich(
            &ingredients,
            ethers::utils::parse_ether("50").unwrap(),
            &testhelper::get_next_block_info(fork_block_num, &ws_provider).await,
            &mut db,
            &SandwichMaker::new().await,
        )
        .await
        {
            Ok(sandwich) => println!("revenue: {:?}", sandwich.revenue),
            Err(_) => println!("not sandwichable"),
        };
        println!("total_duration took: {:?}", start.elapsed());
    }

    #[test]
    fn sandv2_uniswap_router_reverse_pepe() {
        let rt = Runtime::new().unwrap();
        // pepe
        rt.block_on(async {
            create_test_reverse(
                17878004,
                "0xdbe94db12fc555e891717eb0fd2e34cf72a49644",
                vec!["0x12ae088914416bca1f8189369e60d77561c917c10549e0188eb6ff379f53ef57"],
                true,
                "sandv2_uniswap_router_reverse_pepe",
            )
            .await;
        });
    }

    #[test]
    fn sandv2_uniswap_router_reverse_dydx() {
        let rt = Runtime::new().unwrap();

        // dydx
        rt.block_on(async {
            create_test_reverse(
                17805264,
                "0xf660809b6d2d34cc43f620a9b22a40895365a5f8",
                vec!["0x586b48227b5a6cd553b11c0446441121d4667b9c57fee6baf752893c3b2242c6"],
                true,
                "sandv2_uniswap_router_reverse_dydx",
            )
            .await;
        });
    }

    #[test]
    fn sandv2_uniswap_router_reverse_x() {
        let rt = Runtime::new().unwrap();
        // x
        rt.block_on(async {
            create_test_reverse(
                17882689,
                "0x60a8ea6005f7db580bc0c9341e7e6275d114e874",
                vec!["0x913ccc34fbe5480736368f2972b1738d0be527d9237698deea2c14e999207d08"],
                true,
                "sandv2_uniswap_router_reverse_x",
            )
            .await;
        });
    }

    #[test]
    fn sandv2_uniswap_router_reverse_edge() {
        let rt = Runtime::new().unwrap();
        // edge
        rt.block_on(async {
            create_test_reverse(
                17706152,
                "0xa4c13470da60e81f15d304d071a9e1168605a6e0",
                vec!["0x51518a6cbdc86b85468e405cef66a451377d04dee4a04eddfa1c9463569eea8b"],
                true,
                "sandv2_uniswap_router_reverse_edge",
            )
            .await;
        });
    }

    #[test]
    fn sandv2_uniswap_router_reverse_ydf() {
        let rt = Runtime::new().unwrap();
        // ydf
        rt.block_on(async {
            create_test_reverse(
                17882685,
                "0x153f2044feace1eb377c6e1cf644d12677bd86fd",
                vec!["0x32fb942763fc378512a0ab648ecc70e1de1acf8aaf12a47a25018677420adeba"],
                true,
                "sandv2_uniswap_router_reverse_ydf",
            )
            .await;
        });
    }

    #[test]
    fn sandv2_uniswap_router_reverse_futu() {
        let rt = Runtime::new().unwrap();
        // futu
        rt.block_on(async {
            create_test_reverse(
                17882365,
                "0x56e73243101bc0bdbf3bae0bd5db0bea94c5251d",
                vec!["0xcaabd7dc3454a3cfd5a06e768cc6409aeefac17b03e4fc97648679627ef80a74"],
                true,
                "sandv2_uniswap_router_reverse_futu",
            )
            .await;
        });
    }

    #[test]
    fn sandv2_uniswap_router_reverse_datboi() {
        let rt = Runtime::new().unwrap();
        // datboi
        rt.block_on(async {
            create_test_reverse(
                17882365,
                "0x076d3f4cdb47dc8bb223e7ad656c9c4c041b7353",
                vec!["0xcaabd7dc3454a3cfd5a06e768cc6409aeefac17b03e4fc97648679627ef80a74"],
                true,
                "sandv2_uniswap_router_reverse_datboi",
            )
            .await;
        });
    }
    
    #[test]
    fn sandv2_uniswap_router_reverse_bad() {
        let rt = Runtime::new().unwrap();
        // bad(available)
        rt.block_on(async {
            create_test_reverse(
                17864648,
                "0x29c830864930c897efa2b9e9851342187b82010e",
                // vec!["0x472b923421a68f9fdeeceb0c57c35b3908ec3f5bc8ebabe8193057f6dd2a6a9a"],
                vec!["0xf9944763d2c639e98c9df584c1e76e1ed10f912a28c8f062654bb096370e4dd0",
                            "0x7e6745dcf989730e2230ed80973cbabdb253f7d2cde0fd4fc49a233e3dfa8940"],
                true,
                "sandv2_uniswap_router_reverse_bad",
            )
            .await;
        });
    }
    
    #[test]
    fn sandv2_uniswap_router_reverse_crypto() {
        let rt = Runtime::new().unwrap();
        // crypto
        rt.block_on(async {
            create_test_reverse(
                17887686,
                "0x6cea05f7cb348d48a0bdf86889040f6a5bae98dd",
                vec!["0xf4d4e520f753ca3cb5e74647990ca3a3f1a57bd6840fca556ef3a240f264ad6e"],
                true,
                "sandv2_uniswap_router_reverse_crypto",
            )
            .await;
        });
    }

    #[test]
    fn sandv2_uniswap_router_reverse_mog() {
        let rt = Runtime::new().unwrap();
        // mog
        rt.block_on(async {
            create_test_reverse(
                17888040,
                "0xc2eab7d33d3cb97692ecb231a5d0e4a649cb539d",
                vec!["0xbffab04e9a51c97f1fe5ac1266dd5504ff540651e48112908068ca018a107817"],
                true,
                "sandv2_uniswap_router_reverse_mog",
            )
            .await;
        });
    }


    #[test]
    fn sandv2_sushi_router() {
        // Can't use [tokio::test] attr with `global_backed` for some reason
        // so manually create a runtime
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            create_test(
                17721757,
                "0x7fdeb46b3a0916630f36e886d675602b1007fcbb",
                vec!["0xbf5aaafe4d8e1eba3dfb1d05e9b45e9532bd8cb58e1e00a0679041e4bee6c1d0"],
                true,
            )
            .await;
        });
    }

    #[test]
    fn sandv3_uniswap_universal_router_one() {
        // Can't use [tokio::test] attr with `global_backed` for some reason
        // so manually create a runtime
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            create_test(
                16863224,
                "0x62CBac19051b130746Ec4CF96113aF5618F3A212",
                vec!["0x90dfe56814821e7f76f2e4970a7b35948670a968abffebb7be69fe528283e6d8"],
                false,
            )
            .await;
        });
    }

    #[test]
    fn sandv3_uniswap_universal_router_two() {
        // Can't use [tokio::test] attr with `global_backed` for some reason
        // so manually create a runtime
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            create_test(
                16863008,
                "0xa80838D2BB3d6eBaEd1978FA23b38F91775D8378",
                vec!["0xcb0d4dc905ae0662e5f18b4ad0c2af4e700e8b5969d878a2dcfd0d9507435f4d"],
                false,
            )
            .await;
        });
    }

    #[test]
    fn sandv2_kyber_swap() {
        // Can't use [tokio::test] attr with `global_backed` for some reason
        // so manually create a runtime
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            create_test(
                16863312,
                "0x08650bb9dc722C9c8C62E79C2BAfA2d3fc5B3293",
                vec!["0x907894174999fdddc8d8f8e90c210cdb894b91c2c0d79ac35603007d3ce54d00"],
                true,
            )
            .await;
        });
    }

    #[test]
    fn sandv2_non_sandwichable() {
        // Can't use [tokio::test] attr with `global_backed` for some reason
        // so manually create a runtime
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            create_test(
                16780624,
                "0x657c6a08d49b4f0778f9cce1dc49d196cfce9d08",
                vec!["0x77b0b15a3216885a66b3b800173e0edcae9d8d191f7093b99a46fc9346f67466"],
                true,
            )
            .await;
        });
    }

    #[test]
    fn sandv2_multi_with_three_expect_one_reverts() {
        // Can't use [tokio::test] attr with `global_backed` for some reason
        // so manually create a runtime
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            create_test(
                16780624,
                "0x657c6a08d49b4f0778f9cce1dc49d196cfce9d08",
                vec![
                    "0x4791d05bdd6765f036ff4ae44fc27099997417e3bdb053ecb52182bbfc7767c5",
                    "0x923c9ba97fea8d72e60c14d1cc360a8e7d99dd4b31274928d6a79704a8546eda",
                    "0x77b0b15a3216885a66b3b800173e0edcae9d8d191f7093b99a46fc9346f67466",
                ],
                true,
            )
            .await;
        });
    }

    #[test]
    fn sandv2_multi_two() {
        // Can't use [tokio::test] attr with `global_backed` for some reason
        // so manually create a runtime
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            create_test(
                16780624,
                "0x657c6a08d49B4F0778f9cce1Dc49d196cFCe9d08",
                vec![
                    "0x4791d05bdd6765f036ff4ae44fc27099997417e3bdb053ecb52182bbfc7767c5",
                    "0x923c9ba97fea8d72e60c14d1cc360a8e7d99dd4b31274928d6a79704a8546eda",
                ],
                true,
            )
            .await;
        });
    }

    #[test]
    fn sandv2_metamask_swap_router() {
        // Can't use [tokio::test] attr with `global_backed` for some reason
        // so manually create a runtime
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            create_test(
                16873743,
                "0x7A9dDcf06260404D14AbE3bE99c1804D2A5239ce",
                vec!["0xcce01725bf7abfab3a4a533275cb4558a66d7794153b4ec01debaf5abd0dc21f"],
                true,
            )
            .await;
        });
    }
}
