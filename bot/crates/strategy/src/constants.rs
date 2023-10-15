use ethers::{
    prelude::Lazy,
    types::{Address, Bytes, H256, U256},
};
use foundry_evm::revm::primitives::{B160 as rAddress, U256 as rU256};

pub static ONE_ETHER_IN_WEI: Lazy<rU256> = Lazy::new(|| rU256::from(1000000000000000000_u128));
pub static WETH_FUND_AMT: Lazy<rU256> = Lazy::new(|| rU256::from(69) * *ONE_ETHER_IN_WEI);

pub static SEARCHER_WETH_AMT: u128 = 10;
pub static FUND_OTHER_AMT_BASE: u128 = 9999999;

pub static MAX_DIFF_RATE_OF_ONE_ETHER: u128 = 500_000;
pub static MIN_REVENUE_THRESHOLD: Lazy<U256> = Lazy::new(|| U256::from(700000));

// could generate random address to use at runtime
pub static LIL_ROUTER_CONTROLLER: Lazy<rAddress> = Lazy::new(|| {
    "0xC0ff33C0ffeeC0ff33C0ffeeC0ff33C0ff33C0ff"
        .parse()
        .unwrap()
});

// could generate random address to use at runtime
pub static LIL_ROUTER_ADDRESS: Lazy<rAddress> = Lazy::new(|| {
    "0xDecafC0ffee15BadDecafC0ffee15BadDecafC0f"
        .parse()
        .unwrap()
});

// could compile from `../contract` at runtime instead of parsing from string
pub static LIL_ROUTER_CODE: Lazy<Bytes> = Lazy::new(|| {
    "0x608060405234801561001057600080fd5b50600436106100415760003560e01c80634b588d401461004657806381eeb93c14610072578063fa461e3314610085575b600080fd5b610059610054366004610743565b61009a565b6040805192835260208301919091520160405180910390f35b610059610080366004610743565b61021f565b610098610093366004610796565b6104e3565b005b600080846001600160a01b038085169086161082816100cd5773fffd8963efd1fc6a506488495d951d5263988d256100d4565b6401000276ad5b9050600082886040516020016100ff92919091151582526001600160a01b0316602082015260400190565b6040516020818303038152906040529050600080856001600160a01b031663128acb0830878f88886040518663ffffffff1660e01b8152600401610147959493929190610863565b60408051808303816000875af1158015610165573d6000803e3d6000fd5b505050506040513d601f19601f82011682018060405250810190610189919061089e565b9150915084610198578161019a565b805b6101a3906108d8565b6040516370a0823160e01b81523060048201529098506001600160a01b038a16906370a0823190602401602060405180830381865afa1580156101ea573d6000803e3d6000fd5b505050506040513d601f19601f8201168201806040525081019061020e91906108f4565b965050505050505094509492505050565b60405163a9059cbb60e01b81526001600160a01b03848116600483015260248201869052600091829185169063a9059cbb906044016020604051808303816000875af1158015610273573d6000803e3d6000fd5b505050506040513d601f19601f82011682018060405250810190610297919061091b565b50600080600080886001600160a01b0316630902f1ac6040518163ffffffff1660e01b8152600401606060405180830381865afa1580156102dc573d6000803e3d6000fd5b505050506040513d601f19601f82011682018060405250810190610300919061095b565b506001600160701b031691506001600160701b03169150866001600160a01b0316886001600160a01b0316101561033c57819350809250610343565b8093508192505b50506040516370a0823160e01b81526001600160a01b03888116600483015260009184918916906370a0823190602401602060405180830381865afa158015610390573d6000803e3d6000fd5b505050506040513d601f19601f820116820180604052508101906103b491906108f4565b6103be91906109ab565b90506103cb818484610606565b9450600080876001600160a01b0316896001600160a01b0316106103f1578660006103f5565b6000875b6040805160008152602081019182905263022c0d9f60e01b90915291935091506001600160a01b038b169063022c0d9f9061043990859085903090602481016109c2565b600060405180830381600087803b15801561045357600080fd5b505af1158015610467573d6000803e3d6000fd5b50506040516370a0823160e01b81523060048201526001600160a01b038b1692506370a082319150602401602060405180830381865afa1580156104af573d6000803e3d6000fd5b505050506040513d601f19601f820116820180604052508101906104d391906108f4565b9550505050505094509492505050565b60008413806104f25750600083135b6104fb57600080fd5b60008061050a838501856109f9565b91509150811561058b5760405163a9059cbb60e01b8152336004820152602481018790526001600160a01b0382169063a9059cbb906044016020604051808303816000875af1158015610561573d6000803e3d6000fd5b505050506040513d601f19601f82011682018060405250810190610585919061091b565b506105fe565b60405163a9059cbb60e01b8152336004820152602481018690526001600160a01b0382169063a9059cbb906044016020604051808303816000875af11580156105d8573d6000803e3d6000fd5b505050506040513d601f19601f820116820180604052508101906105fc919061091b565b505b505050505050565b60008084116106705760405162461bcd60e51b815260206004820152602b60248201527f556e697377617056324c6962726172793a20494e53554646494349454e545f4960448201526a1394155517d05353d5539560aa1b60648201526084015b60405180910390fd5b6000831180156106805750600082115b6106dd5760405162461bcd60e51b815260206004820152602860248201527f556e697377617056324c6962726172793a20494e53554646494349454e545f4c604482015267495155494449545960c01b6064820152608401610667565b60006106eb856103e5610a32565b905060006106f98483610a32565b905060008261070a876103e8610a32565b6107149190610a51565b90506107208183610a69565b979650505050505050565b6001600160a01b038116811461074057600080fd5b50565b6000806000806080858703121561075957600080fd5b84359350602085013561076b8161072b565b9250604085013561077b8161072b565b9150606085013561078b8161072b565b939692955090935050565b600080600080606085870312156107ac57600080fd5b8435935060208501359250604085013567ffffffffffffffff808211156107d257600080fd5b818701915087601f8301126107e657600080fd5b8135818111156107f557600080fd5b88602082850101111561080757600080fd5b95989497505060200194505050565b6000815180845260005b8181101561083c57602081850181015186830182015201610820565b8181111561084e576000602083870101525b50601f01601f19169290920160200192915050565b6001600160a01b0386811682528515156020830152604082018590528316606082015260a06080820181905260009061072090830184610816565b600080604083850312156108b157600080fd5b505080516020909101519092909150565b634e487b7160e01b600052601160045260246000fd5b6000600160ff1b82016108ed576108ed6108c2565b5060000390565b60006020828403121561090657600080fd5b5051919050565b801515811461074057600080fd5b60006020828403121561092d57600080fd5b81516109388161090d565b9392505050565b80516001600160701b038116811461095657600080fd5b919050565b60008060006060848603121561097057600080fd5b6109798461093f565b92506109876020850161093f565b9150604084015163ffffffff811681146109a057600080fd5b809150509250925092565b6000828210156109bd576109bd6108c2565b500390565b84815283602082015260018060a01b03831660408201526080606082015260006109ef6080830184610816565b9695505050505050565b60008060408385031215610a0c57600080fd5b8235610a178161090d565b91506020830135610a278161072b565b809150509250929050565b6000816000190483118215151615610a4c57610a4c6108c2565b500290565b60008219821115610a6457610a646108c2565b500190565b600082610a8657634e487b7160e01b600052601260045260246000fd5b50049056fea2646970667358221220a3830fddb415d84a0f9225a1e9bbeef724e1b5a2dc0efc456635debefba7af2c64736f6c634300080f0033"
        .parse()
        .unwrap()
});

// funciton signature for getting reserves
pub static GET_RESERVES_SIG: Lazy<Bytes> = Lazy::new(|| "0x0902f1ac".parse().unwrap());

pub static ERC20_TRANSFER_EVENT_SIG: Lazy<H256> = Lazy::new(|| {
    "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"
        .parse()
        .unwrap()
});

pub static WETH_ADDRESS: Lazy<Address> = Lazy::new(|| {
    "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
        .parse()
        .unwrap()
});

// when we need an address with a lot of eth
pub static SUGAR_DADDY: Lazy<Address> = Lazy::new(|| {
    "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045"
        .parse()
        .unwrap()
});

// could generate random address to use at runtime
pub static COINBASE: Lazy<rAddress> = Lazy::new(|| {
    "0x690B9A9E9aa1C9dB991C7721a92d351Db4FaC990"
        .parse()
        .unwrap()
});

// pub static DUST_OVERPAY: Lazy<U256> = Lazy::new(|| ethers::utils::parse_ether("0.00015").unwrap());
pub static DUST_OVERPAY: Lazy<U256> = Lazy::new(|| ethers::utils::parse_ether("0.0003861").unwrap());
