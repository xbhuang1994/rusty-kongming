use std::sync::Arc;

use anyhow::Result;
use artemis_core::{
    collectors::{block_collector::BlockCollector, mempool_collector::MempoolCollector},
    engine::Engine,
    executors::flashbots_executor::FlashbotsExecutor,
    types::{CollectorMap, ExecutorMap},
};
use ethers::providers::{Provider, Ws};

use once_cell::sync::OnceCell;

use log::info;
use reqwest::Url;
use rusty_sando::{
    config::Config,
    initialization::{print_banner, setup_logger},
};
use strategy::{
    bot::SandoBot,
    types::{Action, Event, StratConfig},
};
use num_cpus;
use runtime::dynamic_config;
use op_sidecar::echo::tcp_server;

#[tokio::main]
async fn main() -> Result<()> {

    // Setup
    setup_logger()?;
    print_banner();

    // Make config
    static CONFIG: OnceCell<Config> = OnceCell::new();
    let config = Config::read_from_dotenv().await.unwrap();
    let _ = CONFIG.set(config);

    // Init dynamic config
    dynamic_config::init_config();
    info!("Init Dynamic config");

    // Start sidecar server
    let sidecar_listen_address= CONFIG.get().unwrap().sidecar_listen_address.clone();
    tcp_server::start_sidecar_server(sidecar_listen_address.clone()).await?;
    info!("Start Sidecar Server, Listen At {}", sidecar_listen_address);
    
    // Setup ethers provider
    static WS: OnceCell<Ws> = OnceCell::new();
    let ws = Ws::connect(CONFIG.get().unwrap().wss_rpc.clone()).await.unwrap();
    let _ = WS.set(ws);
    
    static PROVIDER: OnceCell<Arc<Provider<Ws>>> = OnceCell::new();
    let provider: Arc<Provider<Ws>> = Arc::new(Provider::new(WS.get().unwrap().clone()));
    let _ = PROVIDER.set(provider);

    // Setup signers
    let flashbots_signer = CONFIG.get().unwrap().bundle_signer.clone();
    // let searcher_signer = config.searcher_signer;

    // Create engine
    let mut engine: Engine<Event, Action> = Engine::default();

    // Setup block collector
    let block_collector = Box::new(BlockCollector::new(PROVIDER.get().unwrap().clone()));
    let block_collector = CollectorMap::new(block_collector, Event::NewBlock);
    engine.add_collector(Box::new(block_collector));

    // Setup mempool collector
    let mempool_collector = Box::new(MempoolCollector::new(PROVIDER.get().unwrap().clone()));
    let mempool_collector = CollectorMap::new(mempool_collector, Event::NewTransaction);
    engine.add_collector(Box::new(mempool_collector));

    // Setup strategy
    static CONFIGS: OnceCell<StratConfig> = OnceCell::new();
    let configs = StratConfig {
        sando_address: CONFIG.get().unwrap().sando_address,
        sando_inception_block: CONFIG.get().unwrap().sando_inception_block,
        searcher_signer: CONFIG.get().unwrap().searcher_signer.clone(),
    };
    let _ = CONFIGS.set(configs);

    // static STRATEGY: Lazy<SandoBot<Provider<Ws>>> = Lazy::new(|| SandoBot::new(provider.clone(), configs));
    // STRATEGY.start_auto_process(8, 1);
    static STRATEGY: OnceCell<SandoBot<Provider<Ws>>> = OnceCell::new();
    let strategy = SandoBot::new(PROVIDER.get().unwrap().clone(), CONFIGS.get().unwrap(), true);
    let _ = STRATEGY.set(strategy);
    // let tt: &dyn Strategy<Event, Action> = STRATEGY.get().unwrap() as &dyn Strategy<Event, Action>;
    // engine.add_strategy(Box::new((STRATEGY.get().unwrap() as &dyn Strategy<Event, Action>)));
    // let p = **Box::new(STRATEGY.get().unwrap());
    engine.add_strategy(Arc::new(STRATEGY.get().unwrap()));

    // get cpu core number
    let cpu_num = num_cpus::get() as i32;
    let _ = STRATEGY.get().unwrap().start_auto_process(cpu_num, 2, 4, 2).await?;

    // Setup flashbots executor
    let executor = Box::new(FlashbotsExecutor::new(
        PROVIDER.get().unwrap().clone(),
        flashbots_signer,
        Url::parse("https://relay.flashbots.net")?,
        CONFIG.get().unwrap().bundle_send_flag.clone(),
    ));
    let executor = ExecutorMap::new(executor, |action| match action {
        Action::SubmitToFlashbots(bundle) => Some(bundle),
    });
    engine.add_executor(Box::new(executor));

    // Start engine
    if let Ok(mut set) = engine.run().await {
        while let Some(res) = set.join_next().await {
            info!("res: {:?}", res)
        }
    }

    Ok(())
}
