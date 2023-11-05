use std::{str::FromStr, sync::Arc};

use cfmms::pool::{Pool, UniswapV2Pool, UniswapV3Pool};
use ethers::{
    prelude::Lazy,
    providers::{Middleware, Provider, Ws},
    types::{Address, Transaction, TxHash, U64},
};
use strategy::{
    bot::SandoBot,
    types::{BlockInfo, RawIngredients, StratConfig, SandwichSwapType},
};

// -- consts --
static WSS_RPC: &str = "ws://65.21.224.37:8545";
pub static WETH_ADDRESS: Lazy<Address> = Lazy::new(|| {
    "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
        .parse()
        .unwrap()
});

pub static SANDO_ADDRESS: Lazy<Address> = Lazy::new(|| {
    "0xaAaAaAaaAaAaAaaAaAAAAAAAAaaaAaAaAaaAaaAa"
        .parse()
        .unwrap()
});
pub static SEARCHER_SIGNER: Lazy<ethers::signers::LocalWallet> = Lazy::new(|| {
    "0000000000000000000000000000000000000000000000000000000000000001"
        .parse::<ethers::signers::LocalWallet>()
        .unwrap()
});

// -- utils --
fn setup_logger() {
    let _ = fern::Dispatch::new()
        .level(log::LevelFilter::Error)
        .level_for("strategy", log::LevelFilter::Info)
        .chain(std::io::stdout())
        .apply();
}

async fn setup_bot(provider: Arc<Provider<Ws>>) -> SandoBot<Provider<Ws>> {
    setup_logger();

    let strat_config = StratConfig {
        sando_address: SANDO_ADDRESS.clone(),
        sando_inception_block: U64::from(17700000),
        searcher_signer: SEARCHER_SIGNER.clone(),
    };

    SandoBot::new(provider, &strat_config, false)
}

async fn block_num_to_info(block_num: u64, provider: Arc<Provider<Ws>>) -> BlockInfo {
    let block = provider.get_block(block_num).await.unwrap().unwrap();
    block.try_into().unwrap()
}

fn hex_to_address(hex: &str) -> Address {
    hex.parse().unwrap()
}

async fn hex_to_univ2_pool(hex: &str, provider: Arc<Provider<Ws>>) -> Pool {
    let pair_address = hex_to_address(hex);
    let pool = UniswapV2Pool::new_from_address(pair_address, provider)
        .await
        .unwrap();
    Pool::UniswapV2(pool)
}

async fn hex_to_univ3_pool(hex: &str, provider: Arc<Provider<Ws>>) -> Pool {
    let pair_address = hex_to_address(hex);
    let pool = UniswapV3Pool::new_from_address(pair_address, provider)
        .await
        .unwrap();
    Pool::UniswapV3(pool)
}

async fn victim_tx_hash(tx: &str, provider: Arc<Provider<Ws>>) -> Transaction {
    let tx_hash: TxHash = TxHash::from_str(tx).unwrap();
    provider.get_transaction(tx_hash).await.unwrap().unwrap()
}

/// testing against: https://eigenphi.io/mev/ethereum/tx/0x292156c07794bc50952673bf948b90ab71148b81938b6ab4904096adb654d99a
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn can_sandwich_uni_v2() {
    let client = Arc::new(Provider::new(Ws::connect(WSS_RPC).await.unwrap()));

    let bot = setup_bot(client.clone()).await;

    let ingredients = RawIngredients::new(
        vec![],
        vec![
            victim_tx_hash(
                "0xfecf2c78d1418e6905c18a6a6301c9d39b14e5320e345adce52baaecf805580d",
                client.clone(),
            )
            .await,
        ],
        *WETH_ADDRESS,
        hex_to_address("0x3642Cf76c5894B4aB51c1080B2c4F5B9eA734106"),
        hex_to_univ2_pool("0x5d1dd0661E1D22697943C1F50Cc726eA3143329b", client.clone()).await,
    );

    let target_block = block_num_to_info(17754167, client.clone()).await;

    let _ = bot
        .is_sandwichable(ingredients, target_block, SandwichSwapType::Forward, false, false)
        .await
        .unwrap();
}

/// testing against: https://eigenphi.io/mev/ethereum/tx/0x056ede919e31be59b7e1e8676b3be1272ce2bbd3d18f42317a26a3d1f2951fc8
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn can_sandwich_sushi_swap() {
    let client = Arc::new(Provider::new(Ws::connect(WSS_RPC).await.unwrap()));

    let bot = setup_bot(client.clone()).await;

    let ingredients = RawIngredients::new(
        vec![],
        vec![
            victim_tx_hash(
                "0xb344fdc6a3b7c65c5dd971cb113567e2ee6d0636f261c3b8d624627b90694cdb",
                client.clone(),
            )
            .await,
        ],
        *WETH_ADDRESS,
        hex_to_address("0x3b484b82567a09e2588A13D54D032153f0c0aEe0"),
        hex_to_univ2_pool("0xB84C45174Bfc6b8F3EaeCBae11deE63114f5c1b2", client.clone()).await,
    );

    let target_block = block_num_to_info(16873148, client.clone()).await;

    let _ = bot
        .is_sandwichable(ingredients, target_block, SandwichSwapType::Forward, false, false)
        .await
        .unwrap();
}

/// testing against: https://eigenphi.io/mev/ethereum/tx/0xc132e351e8c7d3d8763a894512bd8a33e4ca60f56c0516f7a6cafd3128bd59bb
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn can_sandwich_multi_v2_swaps() {
    let client = Arc::new(Provider::new(Ws::connect(WSS_RPC).await.unwrap()));

    let bot = setup_bot(client.clone()).await;

    let ingredients = RawIngredients::new(
        vec![],
        vec![
            victim_tx_hash(
                "0x4791d05bdd6765f036ff4ae44fc27099997417e3bdb053ecb52182bbfc7767c5",
                client.clone(),
            )
            .await,
            victim_tx_hash(
                "0x923c9ba97fea8d72e60c14d1cc360a8e7d99dd4b31274928d6a79704a8546eda",
                client.clone(),
            )
            .await,
        ],
        *WETH_ADDRESS,
        hex_to_address("0x31b16Ff7823096a227Aac78F1C094525A84ab64F"),
        hex_to_univ2_pool("0x657c6a08d49B4F0778f9cce1Dc49d196cFCe9d08", client.clone()).await,
    );

    let target_block = block_num_to_info(16780625, client.clone()).await;

    let _ = bot
        .is_sandwichable(ingredients, target_block, SandwichSwapType::Forward, false, false)
        .await
        .unwrap();
}

/// testing against: https://eigenphi.io/mev/ethereum/tx/0x64158690880d053adc2c42fbadd1838bc6d726cb81982443be00f83b51d8c25d
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn can_sandwich_uni_v3() {
    let client = Arc::new(Provider::new(Ws::connect(WSS_RPC).await.unwrap()));

    let bot = setup_bot(client.clone()).await;

    let ingredients = RawIngredients::new(
        vec![],
        vec![
            victim_tx_hash(
                "0x90dfe56814821e7f76f2e4970a7b35948670a968abffebb7be69fe528283e6d8",
                client.clone(),
            )
            .await,
        ],
        *WETH_ADDRESS,
        hex_to_address("0x24C19F7101c1731b85F1127EaA0407732E36EcDD"),
        hex_to_univ3_pool("0x62CBac19051b130746Ec4CF96113aF5618F3A212", client.clone()).await,
    );

    let target_block = block_num_to_info(16863225, client.clone()).await;

    let _ = bot
        .is_sandwichable(ingredients, target_block, SandwichSwapType::Forward, false, false)
        .await
        .unwrap();
}

/// testing against: https://eigenphi.io/mev/ethereum/tx/0x750208b35c38de8a807f2b4ba971e5215cf3e828be8ddd4963464ea1e8c786ed
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn can_reverse_sandwich_uni_v2() {
    let client = Arc::new(Provider::new(Ws::connect(WSS_RPC).await.unwrap()));

    let bot = setup_bot(client.clone()).await;

    let ingredients = RawIngredients::new(
        vec![],
        vec![
            victim_tx_hash(
                "0x84b92bc769f1292c15cbaec2773db1cee1ba983b20e52a9b0140a2fbf04117b0",
                client.clone(),
            )
            .await,
        ],
        hex_to_address("0x2654e753424a9f02df31cfbc6c5af14a87b6cab5"),
        *WETH_ADDRESS,
        hex_to_univ2_pool("0xe55fe78e41c01df97ae3f2a885b4e65b4f5fe027", client.clone()).await,
    );

    let target_block = block_num_to_info(17926193, client.clone()).await;

    let _ = bot
        .is_sandwichable(ingredients, target_block, SandwichSwapType::Reverse, false, false)
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn can_another_reverse_sandwich_uni_v2() {
    let client = Arc::new(Provider::new(Ws::connect(WSS_RPC).await.unwrap()));

    let bot = setup_bot(client.clone()).await;

    let ingredients = RawIngredients::new(
        vec![],
        vec![
            victim_tx_hash(
                "0xa97c3b15f8bb0903b0472b010c8c91055d6d61dd2f055f9e0cd6948eb0eb28df",
                client.clone(),
            )
            .await,
        ],
        hex_to_address("0x4dfae3690b93c47470b03036a17b23c1be05127c"),
        *WETH_ADDRESS,
        hex_to_univ2_pool("0xaa9b647f42858f2db441f0aa75843a8e7fd5aff2", client.clone()).await,
    );

    let target_block = block_num_to_info(18346682, client.clone()).await;

    let _ = bot
        .is_sandwichable(ingredients, target_block, SandwichSwapType::Reverse, false, false)
        .await
        .unwrap();
}

// https://etherscan.io/txs?block=18447072&p=4
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn can_make_huge_overlay_recpie_sandwich() {

    let client = Arc::new(Provider::new(Ws::connect(WSS_RPC).await.unwrap()));
    let bot = setup_bot(client.clone()).await;
    let target_block = block_num_to_info(18447072, client.clone()).await;

    let ingredients_01 = RawIngredients::new(
        vec![],
        vec![
            victim_tx_hash(
                "0xce068dd289912c6d8499439b8a35c690131c540d602bf158559a34792bc28623",
                client.clone(),
            )
            .await,
        ],
        *WETH_ADDRESS,
        hex_to_address("0x8c7ac134ed985367eadc6f727d79e8295e11435c"),
        hex_to_univ2_pool("0xc6e40537215c1e041616478d8cfe312b1847b997", client.clone()).await,
    );

    let ingredients_02 = RawIngredients::new(
        vec![],
        vec![
            victim_tx_hash(
                "0xc56b83384f26fd2542a3c4d8ad756b8ed62f7a92a3c290d547d1ab8f3ef5e529",
                client.clone(),
            )
            .await,
        ],
        *WETH_ADDRESS,
        hex_to_address("0xde47a2460e4b6c36b26919ef9255b4f3f86de0a0"),
        hex_to_univ2_pool("0x0a8e3f1dcf7b28896f5b4fd44430c9b66731647c", client.clone()).await,
    );

    let ingredients_03 = RawIngredients::new(
        vec![],
        vec![
            victim_tx_hash(
                "0x313a527071b227562e15b1a6d669ed82a1976ece007bbb492dd474ec870ad4b1",
                client.clone(),
            )
            .await,
        ],
        *WETH_ADDRESS,
        hex_to_address("0x1dea4f94ed4307f240d027958072c2876543bb74"),
        hex_to_univ2_pool("0x6e45b2cfe2bbb53b34b8db02fb075ed576530206", client.clone()).await,
    );

    let recipe_01 = bot
        .is_sandwichable(ingredients_01, target_block.clone(), SandwichSwapType::Forward, false, false)
        .await
        .unwrap();

    let recipe_02 = bot
        .is_sandwichable(ingredients_02, target_block.clone(), SandwichSwapType::Forward, false, false)
        .await
        .unwrap();

    let recipe_03 = bot
        .is_sandwichable(ingredients_03, target_block.clone(), SandwichSwapType::Forward, false, false)
        .await
        .unwrap();

    let final_recipes = vec![
        recipe_01.clone(),
        recipe_02.clone(),
        recipe_03.clone(),
    ];

    let provider = Arc::new(Provider::new(Ws::connect(WSS_RPC).await.unwrap()));
    let final_recipe = bot.make_huge_recpie(&final_recipes, target_block.clone(), false).await.unwrap();
    let _ = final_recipe.to_fb_bundle(SANDO_ADDRESS.clone(), &SEARCHER_SIGNER, false, provider, true, false, true, true).await;
}

// https://etherscan.io/txs?block=18505093&p=3
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cannot_make_huge_recpie_with_highest_profit() {

    let client = Arc::new(Provider::new(Ws::connect(WSS_RPC).await.unwrap()));
    let bot = setup_bot(client.clone()).await;
    let target_block = block_num_to_info(18505093, client.clone()).await;

    let ingredients_01 = RawIngredients::new(
        vec![],
        vec![
            victim_tx_hash(
                "0x191d4ddd39635e184e649588cd97dc0144baa9e5a1244050ae8e30930c0ea543",
                client.clone(),
            )
            .await,
        ],
        *WETH_ADDRESS,
        hex_to_address("0x52d59ee4154baa82def09f1051df8984e3b3d916"),
        hex_to_univ3_pool("0x4d51a2b38483a6c1213e6712e8b5e01c52113eab", client.clone()).await,
    );

    let ingredients_02 = RawIngredients::new(
        vec![],
        vec![
            victim_tx_hash(
                "0x3f3ab716b71de815e606bf1847c9c4ca16af79338fbb186ae2b288dfd288d281",
                client.clone(),
            )
            .await,
        ],
        *WETH_ADDRESS,
        hex_to_address("0x25a1fa439947799063bbcbcf21a5ba1f77a74299"),
        hex_to_univ2_pool("0x6561d51aa433c2b574fbe95ff515c278f7ff88d7", client.clone()).await,
    );

    let ingredients_03 = RawIngredients::new(
        vec![],
        vec![
            victim_tx_hash(
                "0xeac636d796af5072f5a292abc6219fdd1f2d53df9fcdfaea9381a5ade31acc37",
                client.clone(),
            )
            .await,
        ],
        *WETH_ADDRESS,
        hex_to_address("0xcd24181edaa0394e1d978d5011e36f67fe41a499"),
        hex_to_univ2_pool("0x1e08122b8447679c1a198108a755d17ef09abbd7", client.clone()).await,
    );

    let recipe_low = bot
        .is_sandwichable(ingredients_01, target_block.clone(), SandwichSwapType::Forward, false, false)
        .await
        .unwrap();

    let recipe_optimal_01 = bot
        .is_sandwichable(ingredients_02, target_block.clone(), SandwichSwapType::Forward, false, false)
        .await
        .unwrap();

    let recipe_optimal_02 = bot
        .is_sandwichable(ingredients_03, target_block.clone(), SandwichSwapType::Forward, false, false)
        .await
        .unwrap();

    let optimal_recipes = vec![
        recipe_optimal_01.clone(),
        recipe_optimal_02.clone(),
    ];
    let low_recipes = vec![
        recipe_low.clone(),
    ];

    let provider = Arc::new(Provider::new(Ws::connect(WSS_RPC).await.unwrap()));
    let final_recipe = bot.make_huge_recpie_with_highest_profit(&optimal_recipes, &low_recipes, target_block.clone()).await;
    match final_recipe {
        Some(recipe) => {
            let _ = recipe.to_fb_bundle(SANDO_ADDRESS.clone(), &SEARCHER_SIGNER, false, provider, true, false, true, true).await;
        },
        None => {}
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn can_make_huge_recpie_with_highest_profit() {

    let client = Arc::new(Provider::new(Ws::connect(WSS_RPC).await.unwrap()));
    let bot = setup_bot(client.clone()).await;
    let target_block = block_num_to_info(18506572, client.clone()).await;

    let ingredients_01 = RawIngredients::new(
        vec![],
        vec![
            victim_tx_hash(
                "0x152128ef67798857ceac14921d1be615416d27d312598059df96ba07358769fb",
                client.clone(),
            )
            .await,
        ],
        *WETH_ADDRESS,
        hex_to_address("0x9506d37f70eb4c3d79c398d326c871abbf10521d"),
        hex_to_univ2_pool("0x9b3df8eae6e1ed1b931086852860d3c6375d7ae6", client.clone()).await,
    );

    let ingredients_02 = RawIngredients::new(
        vec![],
        vec![
            victim_tx_hash(
                "0x557197282006a7c0ae839b25ba98002687649aa48bfa1ab460fa0fc132277e0e",
                client.clone(),
            )
            .await,
        ],
        *WETH_ADDRESS,
        hex_to_address("0xcb454adae2595ac182fc1807b0c59ef3f31496be"),
        hex_to_univ2_pool("0x822cd8fb01fe182143499bc5bad64f5b3e75bc03", client.clone()).await,
    );

    let recipe_optimal = bot
        .is_sandwichable(ingredients_01, target_block.clone(), SandwichSwapType::Forward, false, false)
        .await
        .unwrap();

    let recipe_low = bot
        .is_sandwichable(ingredients_02, target_block.clone(), SandwichSwapType::Forward, false, false)
        .await
        .unwrap();

    let optimal_recipes = vec![
        recipe_optimal.clone(),
    ];
    let low_recipes = vec![
        recipe_low.clone(),
    ];

    let provider = Arc::new(Provider::new(Ws::connect(WSS_RPC).await.unwrap()));
    
    let _ = recipe_optimal.to_fb_bundle(SANDO_ADDRESS.clone(), &SEARCHER_SIGNER, false, provider.clone(), true, false, true, true).await;

    let final_recipe = bot.make_huge_recpie_with_highest_profit(&optimal_recipes, &low_recipes, target_block.clone()).await;
    match final_recipe {
        Some(recipe) => {
            let _ = recipe.to_fb_bundle(SANDO_ADDRESS.clone(), &SEARCHER_SIGNER, false, provider.clone(), true, false, true, true).await;
        },
        None => {}
    }
}