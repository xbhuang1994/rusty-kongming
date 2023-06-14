// SPDX-License-Identifier: MIT
pragma solidity ^0.8.15;

import "forge-std/Script.sol";
import "forge-std/console.sol";
import "../src/CPO.sol";


contract DeployCPO is Script {
    CPO cpo;
    address logic1;
    // serachers
    function setUp() public {
        logic1 = 0x687bB6c57915aa2529EfC7D2a26668855e022fAE;
    }
    function run() public{
        string memory name = "feet";
        bytes32 salt = bytes32(uint256(0xf337));
        vm.startBroadcast();
        cpo = new CPO(0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266);
        address proxy = cpo.createProxy(name, salt, logic1);
        // cpo.destroyProxy(name);
        vm.stopBroadcast();
        console.log(address(cpo));
        vm.startBroadcast();
        // Creates the new proxy
        // proxy = cpo.createProxy(name, salt, logic1);
        vm.stopBroadcast();
        console.log(proxy);

    }
}
//run bash
//forge script ./script/DeploySandwich.s.sol --rpc-url http://127.0.0.1:8545 --broadcast --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
