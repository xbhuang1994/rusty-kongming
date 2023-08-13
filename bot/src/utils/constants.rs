use std::str::FromStr;

use ethers::prelude::*;
use indoc::indoc;

use revm::primitives::U256 as rU256;

pub fn get_edge_address() -> Address {
    Address::from_str("0x4ec1b60b96193a64acae44778e51f7bff2007831").unwrap()
}

pub fn get_dydx_address() -> Address {
    Address::from_str("0x92d6c1e31e14520e676a687f0a93788b716beff5").unwrap()
}

pub fn get_x_address() -> Address {
    Address::from_str("0x5f5166c4fdb9055efb24a7e75cc1a21ca8ca61a3").unwrap()
}

pub fn get_pepe_address() -> Address {
    Address::from_str("0x8d5b6a0b8379c660043d841e274d7fe07786a6bc").unwrap()
}

pub fn get_ydf_address() -> Address {
    Address::from_str("0x30dcba0405004cf124045793e1933c798af9e66a").unwrap()
}

pub fn get_futu_address() -> Address {
    Address::from_str("0x86746590604b6b3387905bfc218d4229ec8d7fde").unwrap()
}

pub fn get_datboi_address() -> Address {
    Address::from_str("0x57914df4324a7f9e17062728ca44e566c485af97").unwrap()
}

pub fn get_bad_address() -> Address {
    Address::from_str("0x32b86b99441480a7e5bd3a26c124ec2373e3f015").unwrap()
}

pub fn get_crypto_address() -> Address {
    Address::from_str("0x586a7cfe21e55ec0e24f0bfb118f77fe4ca87bab").unwrap()
}

pub fn get_mog_address() -> Address {
    Address::from_str("0xaaee1a9723aadb7afa2810263653a34ba2c21c7a").unwrap()
}

pub fn get_slot_by_address(addr: Address) -> rU256 {
    if addr == get_edge_address() || addr == get_dydx_address()
        || addr == get_ydf_address() || addr == get_bad_address()
        || addr == get_crypto_address() {
        return rU256::from_str("0xedceff30864d0c59113a29113472807abd04c523799e1d9f56afb2bb1e3410d1").unwrap()
    } else if addr == get_pepe_address() {
        return rU256::from_str("0x2a14a55e6f4c0d1688108845f66780c416401b5154eb4a4f241ea83b3e5e1bda").unwrap()
    } else if addr == get_x_address() {
        return rU256::from_str("0x232deb70cfb822531166001be71bfd7d5255cbbed99199639d5a89e167ffe1ad").unwrap()
    } else if addr == get_futu_address() {
        return rU256::from_str("0x4e02ceddb6d3084053f93bfb0b52f2289e83c4633cfa2e7bfe61df1e52b2ed92").unwrap()
    } else if addr == get_datboi_address() {
        return rU256::from_str("0xa7b01fdf0b7c35d2b6724bac387fdfcee55f2f126845c5c71cd59a2e76ef1fc9").unwrap()
    } else if addr == get_mog_address() {
        return rU256::from_str("0x3eda6cc33a4f384c95b93751d68c3b0e7a97af67e8490ffb0fc26ec84f175998").unwrap()
    } else {
        return rU256::from_str("").unwrap()
    }
}

// Return weth address
pub fn get_weth_address() -> Address {
    Address::from_str("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2").unwrap()
}

// Return the ethdev address (used if we need funds)
pub fn get_eth_dev() -> Address {
    Address::from_str("0x5AbFEc25f74Cd88437631a7731906932776356f9").unwrap()
}

// Returns the bytecode for our custom modded router contract
pub fn get_braindance_code() -> Bytes {
    "608060405234801561001057600080fd5b506004361061004c5760003560e01c80634b588d401461005157806381eeb93c1461007d57806390063e5914610090578063fa461e33146100a5575b600080fd5b61006461005f366004610994565b6100b8565b6040805192835260208301919091520160405180910390f35b61006461008b366004610994565b61023d565b6100a361009e3660046109e7565b610502565b005b6100a36100b3366004610a46565b61061d565b600080846001600160a01b038085169086161082816100eb5773fffd8963efd1fc6a506488495d951d5263988d256100f2565b6401000276ad5b90506000828860405160200161011d92919091151582526001600160a01b0316602082015260400190565b6040516020818303038152906040529050600080856001600160a01b031663128acb0830878f88886040518663ffffffff1660e01b8152600401610165959493929190610b13565b60408051808303816000875af1158015610183573d6000803e3d6000fd5b505050506040513d601f19601f820116820180604052508101906101a79190610b4e565b91509150846101b657816101b8565b805b6101c190610b88565b6040516370a0823160e01b81523060048201529098506001600160a01b038a16906370a0823190602401602060405180830381865afa158015610208573d6000803e3d6000fd5b505050506040513d601f19601f8201168201806040525081019061022c9190610ba4565b965050505050505094509492505050565b60405163a9059cbb60e01b81526001600160a01b03848116600483015260248201869052600091829185169063a9059cbb906044016020604051808303816000875af1158015610291573d6000803e3d6000fd5b505050506040513d601f19601f820116820180604052508101906102b59190610bcb565b50600080600080886001600160a01b0316630902f1ac6040518163ffffffff1660e01b8152600401606060405180830381865afa1580156102fa573d6000803e3d6000fd5b505050506040513d601f19601f8201168201806040525081019061031e9190610c0b565b506001600160701b031691506001600160701b03169150866001600160a01b0316886001600160a01b0316101561035a57819350809250610361565b8093508192505b50506040516370a0823160e01b81526001600160a01b0388811660048301526000916103dd918591908a16906370a0823190602401602060405180830381865afa1580156103b3573d6000803e3d6000fd5b505050506040513d601f19601f820116820180604052508101906103d79190610ba4565b90610740565b90506103ea8184846107a1565b9450600080876001600160a01b0316896001600160a01b03161061041057866000610414565b6000875b6040805160008152602081019182905263022c0d9f60e01b90915291935091506001600160a01b038b169063022c0d9f906104589085908590309060248101610c5b565b600060405180830381600087803b15801561047257600080fd5b505af1158015610486573d6000803e3d6000fd5b50506040516370a0823160e01b81523060048201526001600160a01b038b1692506370a082319150602401602060405180830381865afa1580156104ce573d6000803e3d6000fd5b505050506040513d601f19601f820116820180604052508101906104f29190610ba4565b9550505050505094509492505050565b60405163a9059cbb60e01b81526001600160a01b0384811660048301526024820187905283169063a9059cbb906044016020604051808303816000875af1158015610551573d6000803e3d6000fd5b505050506040513d601f19601f820116820180604052508101906105759190610bcb565b50600080826001600160a01b0316846001600160a01b03161061059a5785600061059e565b6000865b6040805160008152602081019182905263022c0d9f60e01b90915291935091506001600160a01b0386169063022c0d9f906105e29085908590309060248101610c5b565b600060405180830381600087803b1580156105fc57600080fd5b505af1158015610610573d6000803e3d6000fd5b5050505050505050505050565b600084138061062c5750600083135b61063557600080fd5b60008061064483850185610c92565b9150915081156106c55760405163a9059cbb60e01b8152336004820152602481018790526001600160a01b0382169063a9059cbb906044016020604051808303816000875af115801561069b573d6000803e3d6000fd5b505050506040513d601f19601f820116820180604052508101906106bf9190610bcb565b50610738565b60405163a9059cbb60e01b8152336004820152602481018690526001600160a01b0382169063a9059cbb906044016020604051808303816000875af1158015610712573d6000803e3d6000fd5b505050506040513d601f19601f820116820180604052508101906107369190610bcb565b505b505050505050565b60008261074d8382610ccb565b915081111561079b5760405162461bcd60e51b815260206004820152601560248201527464732d6d6174682d7375622d756e646572666c6f7760581b60448201526064015b60405180910390fd5b92915050565b60008084116108065760405162461bcd60e51b815260206004820152602b60248201527f556e697377617056324c6962726172793a20494e53554646494349454e545f4960448201526a1394155517d05353d5539560aa1b6064820152608401610792565b6000831180156108165750600082115b6108735760405162461bcd60e51b815260206004820152602860248201527f556e697377617056324c6962726172793a20494e53554646494349454e545f4c604482015267495155494449545960c01b6064820152608401610792565b6000610881856103e56108c0565b9050600061088f82856108c0565b905060006108a9836108a3886103e86108c0565b90610927565b90506108b58183610ce2565b979650505050505050565b60008115806108e4575082826108d68183610d04565b92506108e29083610ce2565b145b61079b5760405162461bcd60e51b815260206004820152601460248201527364732d6d6174682d6d756c2d6f766572666c6f7760601b6044820152606401610792565b6000826109348382610d23565b915081101561079b5760405162461bcd60e51b815260206004820152601460248201527364732d6d6174682d6164642d6f766572666c6f7760601b6044820152606401610792565b6001600160a01b038116811461099157600080fd5b50565b600080600080608085870312156109aa57600080fd5b8435935060208501356109bc8161097c565b925060408501356109cc8161097c565b915060608501356109dc8161097c565b939692955090935050565b600080600080600060a086880312156109ff57600080fd5b85359450602086013593506040860135610a188161097c565b92506060860135610a288161097c565b91506080860135610a388161097c565b809150509295509295909350565b60008060008060608587031215610a5c57600080fd5b8435935060208501359250604085013567ffffffffffffffff80821115610a8257600080fd5b818701915087601f830112610a9657600080fd5b813581811115610aa557600080fd5b886020828501011115610ab757600080fd5b95989497505060200194505050565b6000815180845260005b81811015610aec57602081850181015186830182015201610ad0565b81811115610afe576000602083870101525b50601f01601f19169290920160200192915050565b6001600160a01b0386811682528515156020830152604082018590528316606082015260a0608082018190526000906108b590830184610ac6565b60008060408385031215610b6157600080fd5b505080516020909101519092909150565b634e487b7160e01b600052601160045260246000fd5b6000600160ff1b8201610b9d57610b9d610b72565b5060000390565b600060208284031215610bb657600080fd5b5051919050565b801515811461099157600080fd5b600060208284031215610bdd57600080fd5b8151610be881610bbd565b9392505050565b80516001600160701b0381168114610c0657600080fd5b919050565b600080600060608486031215610c2057600080fd5b610c2984610bef565b9250610c3760208501610bef565b9150604084015163ffffffff81168114610c5057600080fd5b809150509250925092565b84815283602082015260018060a01b0383166040820152608060608201526000610c886080830184610ac6565b9695505050505050565b60008060408385031215610ca557600080fd5b8235610cb081610bbd565b91506020830135610cc08161097c565b809150509250929050565b600082821015610cdd57610cdd610b72565b500390565b600082610cff57634e487b7160e01b600052601260045260246000fd5b500490565b6000816000190483118215151615610d1e57610d1e610b72565b500290565b60008219821115610d3657610d36610b72565b50019056fea2646970667358221220acb668db58d51617c0d50e902950ba737188460329e57df8dc4a043d4483bdad64736f6c634300080f0033".parse().unwrap()
    // debug code: just return "amountOut" and "realAfterBalance"
    // "0x608060405234801561001057600080fd5b50600436106100415760003560e01c80634b588d401461004657806381eeb93c14610072578063fa461e3314610085575b600080fd5b61005961005436600461043d565b61009a565b6040805192835260208301919091520160405180910390f35b61005961008036600461043d565b61021f565b610098610093366004610490565b610302565b005b600080846001600160a01b038085169086161082816100cd5773fffd8963efd1fc6a506488495d951d5263988d256100d4565b6401000276ad5b9050600082886040516020016100ff92919091151582526001600160a01b0316602082015260400190565b6040516020818303038152906040529050600080856001600160a01b031663128acb0830878f88886040518663ffffffff1660e01b8152600401610147959493929190610510565b60408051808303816000875af1158015610165573d6000803e3d6000fd5b505050506040513d601f19601f820116820180604052508101906101899190610591565b9150915084610198578161019a565b805b6101a3906105b5565b6040516370a0823160e01b81523060048201529098506001600160a01b038a16906370a0823190602401602060405180830381865afa1580156101ea573d6000803e3d6000fd5b505050506040513d601f19601f8201168201806040525081019061020e91906105df565b965050505050505094509492505050565b6040516370a0823160e01b815230600482015260009081906001600160a01b038516906370a0823190602401602060405180830381865afa158015610268573d6000803e3d6000fd5b505050506040513d601f19601f8201168201806040525081019061028c91906105df565b6040516370a0823160e01b81523060048201529092506001600160a01b038416906370a0823190602401602060405180830381865afa1580156102d3573d6000803e3d6000fd5b505050506040513d601f19601f820116820180604052508101906102f791906105df565b905094509492505050565b60008413806103115750600083135b61031a57600080fd5b60008061032983850185610606565b9150915081156103aa5760405163a9059cbb60e01b8152336004820152602481018790526001600160a01b0382169063a9059cbb906044016020604051808303816000875af1158015610380573d6000803e3d6000fd5b505050506040513d601f19601f820116820180604052508101906103a4919061063f565b5061041d565b60405163a9059cbb60e01b8152336004820152602481018690526001600160a01b0382169063a9059cbb906044016020604051808303816000875af11580156103f7573d6000803e3d6000fd5b505050506040513d601f19601f8201168201806040525081019061041b919061063f565b505b505050505050565b6001600160a01b038116811461043a57600080fd5b50565b6000806000806080858703121561045357600080fd5b84359350602085013561046581610425565b9250604085013561047581610425565b9150606085013561048581610425565b939692955090935050565b600080600080606085870312156104a657600080fd5b8435935060208501359250604085013567ffffffffffffffff808211156104cc57600080fd5b818701915087601f8301126104e057600080fd5b8135818111156104ef57600080fd5b88602082850101111561050157600080fd5b95989497505060200194505050565b600060018060a01b038088168352602087151581850152866040850152818616606085015260a06080850152845191508160a085015260005b828110156105655785810182015185820160c001528101610549565b8281111561057757600060c084870101525b5050601f01601f19169190910160c0019695505050505050565b600080604083850312156105a457600080fd5b505080516020909101519092909150565b6000600160ff1b82016105d857634e487b7160e01b600052601160045260246000fd5b5060000390565b6000602082840312156105f157600080fd5b5051919050565b801515811461043a57600080fd5b6000806040838503121561061957600080fd5b8235610624816105f8565b9150602083013561063481610425565b809150509250929050565b60006020828403121561065157600080fd5b815161065c816105f8565b939250505056fea26469706673582212201a95113fefd4e835ce27705ba7dedbe342eedd0e17984875eb63423aa87c1ca964736f6c634300080f0033".parse().unwrap()
}

// Return runtime code for our sandwich contract (if u want to test new contract impl)
pub fn get_test_sandwich_code() -> Bytes {
    "3d3560001a565b61063e565b610798565b6106eb565b61086a565b6104ec565b6105a1565b61043f565b6103aa565b6102e1565b610230565b61093c565b61095c565b610982565b610a04565b610af0565b610bc8565b610cc1565b610d9e565b610e72565b610f5e560000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005b6099357fff000000000000000000000000000000000000000000000000000000000000006000527f1f98431c8ad98523631ae4a59f267346ea31f98400000000000000000000000046526015527fe34f199b19b2b4f47f68442619d555527d244f78a3297ea89325f843f87b8b54603552605560002073ffffffffffffffffffffffffffffffffffffffff163314156109ee573d3d60443d3d7effffffffffffffffffffffffffffffffffffffff00000000000000000000006084351660581c60843560f81c6101fa577fa9059cbb000000000000000000000000000000000000000000000000000000003d52336004526024356024525af1156109ee57005b7fa9059cbb000000000000000000000000000000000000000000000000000000003d52336004526004356024525af1156109ee57005b60353560e01c4314156109ee57737e5f4552091a69125d5dfcb7b8c2659029395bdf3314156109ee573d3d60f93d3d463560601c7f128acb080000000000000000000000000000000000000000000000000000000060005230600452620186a0340260445273fffd8963efd1fc6a506488495d951d5263988d2560645260a0608452603560a45273c02aaa39b223fe8d0a0e5c4f27ead9083c756cc260581b60c45260153560d9525af1156109ee57005b60353560e01c4314156109ee57737e5f4552091a69125d5dfcb7b8c2659029395bdf3314156109ee573d3d60f93d3d463560601c7f128acb0800000000000000000000000000000000000000000000000000000000600052306004526001602452620186a034026044526401000276ad60645260a0608452603560a4527f010000000000000000000000000000000000000000000000000000000000000073c02aaa39b223fe8d0a0e5c4f27ead9083c756cc260581b0160c45260153560d9525af1156109ee57005b737e5f4552091a69125d5dfcb7b8c2659029395bdf3314156109ee573d3d60f93d3d463560601c7f128acb08000000000000000000000000000000000000000000000000000000006000523060045260293560d01c60445273fffd8963efd1fc6a506488495d951d5263988d2560645260a0608452603560a45260153560601c60581b60c452602f3560d9525af1156109ee57005b737e5f4552091a69125d5dfcb7b8c2659029395bdf3314156109ee573d3d60f93d3d463560601c7f128acb080000000000000000000000000000000000000000000000000000000060005230600452600160245260293560d01c6044526401000276ad60645260a0608452603560a4527f010000000000000000000000000000000000000000000000000000000000000060153560601c60581b0160c452602f3560d9525af1156109ee57005b737e5f4552091a69125d5dfcb7b8c2659029395bdf3314156109ee573d3d60f93d3d463560601c7f128acb080000000000000000000000000000000000000000000000000000000060005230600452600160245260293560b81c6509184e72a000026044526401000276ad60645260a0608452603560a4527f010000000000000000000000000000000000000000000000000000000000000060153560601c60581b0160c45260323560d9525af1156109ee57005b737e5f4552091a69125d5dfcb7b8c2659029395bdf3314156109ee573d3d60f93d3d463560601c7f128acb08000000000000000000000000000000000000000000000000000000006000523060045260293560b81c6509184e72a0000260445273fffd8963efd1fc6a506488495d951d5263988d2560645260a0608452603560a45260153560601c60581b60c45260323560d9525af1156109ee57005b737e5f4552091a69125d5dfcb7b8c2659029395bdf3314156109ee573d3d60a43d3d463560601c3d3d7fa9059cbb000000000000000000000000000000000000000000000000000000003d52826004526029358060081b9060001a5260443d3d60153560601c5af1507f022c0d9f00000000000000000000000000000000000000000000000000000000600052620186a0340260045260006024523060445260806064525af1156109ee57005b737e5f4552091a69125d5dfcb7b8c2659029395bdf3314156109ee573d3d60a43d3d463560601c3d3d7fa9059cbb000000000000000000000000000000000000000000000000000000003d52826004526029358060081b9060001a5260443d3d60153560601c5af1507f022c0d9f000000000000000000000000000000000000000000000000000000006000526000600452620186a034026024523060445260806064525af1156109ee57005b601a3560e01c4314156109ee57737e5f4552091a69125d5dfcb7b8c2659029395bdf3314156109ee573d3d60a43d3d463560601c3d3d7f23b872dd000000000000000000000000000000000000000000000000000000003d523060045282602452620186a0340260445260643d3d73c02aaa39b223fe8d0a0e5c4f27ead9083c756cc25af1507f022c0d9f00000000000000000000000000000000000000000000000000000000600052600060045260006024526015358060081b9060001a523060445260806064525af1156109ee57005b601a3560e01c4314156109ee57737e5f4552091a69125d5dfcb7b8c2659029395bdf3314156109ee573d3d60a43d3d463560601c3d3d7f23b872dd000000000000000000000000000000000000000000000000000000003d523060045282602452620186a0340260445260643d3d73c02aaa39b223fe8d0a0e5c4f27ead9083c756cc25af1507f022c0d9f0000000000000000000000000000000000000000000000000000000060005260006004526015358060081b9060001a5260006024523060445260806064525af1156109ee57005b737e5f4552091a69125d5dfcb7b8c2659029395bdf3314156109ee5733ff005b737e5f4552091a69125d5dfcb7b8c2659029395bdf3314156109ee573d3d3d3d47335af1005b737e5f4552091a69125d5dfcb7b8c2659029395bdf3314156109ee577fa9059cbb0000000000000000000000000000000000000000000000000000000059523360045246356024523d3d60443d3d73c02aaa39b223fe8d0a0e5c4f27ead9083c756cc25af1156109ee57005b600380fd5b803614610a025780345235341a565b005b737e5f4552091a69125d5dfcb7b8c2659029395bdf3314156109ee573451343460a434348546013560601c34347f23b872dd0000000000000000000000000000000000000000000000000000000034523060045282602452886015013560d81c60d81b8060081b90341a604001526064343473c02aaa39b223fe8d0a0e5c4f27ead9083c756cc25af1507f022c0d9f000000000000000000000000000000000000000000000000000000003452346004523460245286601a013560d81c60d81b8060081b90341a523060445260806064525af1156109ee57346024523460445234606452601f016109f3565b737e5f4552091a69125d5dfcb7b8c2659029395bdf3314156109ee573451343460a434348546013560601c34347fa9059cbb00000000000000000000000000000000000000000000000000000000345282600452886029013560d81c60d81b8060081b90341a52604434348b6015013560601c5af1507f022c0d9f000000000000000000000000000000000000000000000000000000003452346004523460245286602e013560d81c60d81b8060081b90341a523060445260806064525af1156109ee573460245234604452346064526033016109f3565b737e5f4552091a69125d5dfcb7b8c2659029395bdf3314156109ee573451343460f934348546013560601c7f128acb0800000000000000000000000000000000000000000000000000000000600052306004526001602452866035013560d81c60d81b8060081b90341a602001526401000276ad60645260a0608452603560a4527f010000000000000000000000000000000000000000000000000000000000000073c02aaa39b223fe8d0a0e5c4f27ead9083c756cc260581b0160c452866015013560d9525af1156109ee5734600452346024523460445234606452346084523460a4523460b4523460c4523460d952603a016109f3565b737e5f4552091a69125d5dfcb7b8c2659029395bdf3314156109ee573451343460f934348546013560601c7f128acb080000000000000000000000000000000000000000000000000000000060005230600452866035013560d81c60d81b8060081b90341a6020015273fffd8963efd1fc6a506488495d951d5263988d2560645260a0608452603560a45273c02aaa39b223fe8d0a0e5c4f27ead9083c756cc260581b60c452866015013560d9525af1156109ee57346004523460445234606452346084523460a4523460b4523460c4523460d952603a016109f3565b737e5f4552091a69125d5dfcb7b8c2659029395bdf3314156109ee573451343460f934348546013560601c7f128acb080000000000000000000000000000000000000000000000000000000060005230600452866035013560d81c60d81b8060081b90341a6020015273fffd8963efd1fc6a506488495d951d5263988d2560645260a0608452603560a45286603a013560601c60581b60c452866015013560d9525af1156109ee5734600452346024523460445234606452346084523460a4523460b4523460c4523460d952604e016109f3565b737e5f4552091a69125d5dfcb7b8c2659029395bdf3314156109ee573451343460f934348546013560601c7f128acb0800000000000000000000000000000000000000000000000000000000600052306004526001602452866035013560d81c60d81b8060081b90341a602001526401000276ad60645260a0608452603560a4527f010000000000000000000000000000000000000000000000000000000000000087603a013560601c60581b0160c452866015013560d9525af1156109ee5734600452346024523460445234606452346084523460a4523460b4523460c4523460d952604e016109f3565b463560e01c4314156109ee5760056109f356".parse().unwrap()
}

// Return the event signature to a erc20 transfer
pub fn get_erc20_transfer_event_signature() -> H256 {
    H256::from_str("0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef").unwrap()
}

pub fn get_banner() -> &'static str {
    let banner = indoc! {
r#"

 _____   ___   _   _ ______  _____         ______  _____
/  ___| / _ \ | \ | ||  _  \|  _  |        | ___ \/  ___|
\ `--. / /_\ \|  \| || | | || | | | ______ | |_/ /\ `--.
 `--. \|  _  || . ` || | | || | | ||______||    /  `--. \
/\__/ /| | | || |\  || |/ / \ \_/ /        | |\ \ /\__/ /
\____/ \_| |_/\_| \_/|___/   \___/         \_| \_|\____/

______ __   __     _____       ___  ___ _____  _   _  _____  _____  _      _____  _____  _____
| ___ \\ \ / / _  |  _  |      |  \/  ||  _  || | | |/  ___||  ___|| |    |  ___|/  ___|/  ___|
| |_/ / \ V / (_) | |/' |__  __| .  . || | | || | | |\ `--. | |__  | |    | |__  \ `--. \ `--.
| ___ \  \ /      |  /| |\ \/ /| |\/| || | | || | | | `--. \|  __| | |    |  __|  `--. \ `--. \
| |_/ /  | |   _  \ |_/ / >  < | |  | |\ \_/ /| |_| |/\__/ /| |___ | |____| |___ /\__/ //\__/ /
\____/   \_/  (_)  \___/ /_/\_\\_|  |_/ \___/  \___/ \____/ \____/ \_____/\____/ \____/ \____/
"#};
    banner
}
