require('dotenv').config();
const ethers = require('ethers');
const shell = require('shelljs');
// const wethContractABI = require('./abi/IWETH.json');
const contractFactoryABI = require('./abi/IMetamorphicContractFactory.json');
// Create an ethers provider instance
const provider = new ethers.providers.JsonRpcProvider('http://localhost:8545');
const wallet = new ethers.Wallet('0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80', provider);
const mywallet = new ethers.Wallet(process.env.DEPOLY_KEY, provider);
// Create a contract instance
const contractAddress = '0x00000000e82eb0431756271F0d00CFB143685e7B'; // Replace with the actual contract address


async function main() {
    const sendAmount = ethers.utils.parseEther('1'); // 发送的ETH数量，此处为0.1 ETH
    const transaction = {
        to: mywallet.address,
        value: sendAmount,
    };
    await wallet.sendTransaction(transaction);
    let balance = await mywallet.getBalance();
    console.log(balance.toString());
    const contractFactory = new ethers.Contract(contractAddress, contractFactoryABI, mywallet);
    const slat = "0x30ce0df88936ecd176af29f63ba3f3c8b978bfdaa05c91e9d6dfe501c745a809";
    if (!shell.which('huffc')) {
        shell.echo('huffc is not installed');
        shell.exit(1);
    }
    let bc = shell.exec('huffc --bytecode ../contract/src/sandwich.huff').stdout;
    const res = await contractFactory.deployMetamorphicContract(slat, "0x" + bc, "0x");
    const rep = await res.wait();
    console.log("\n");
    const metamorphicContract = rep.events[0].args.metamorphicContract;
    console.log("metamorphic contract:", metamorphicContract);


    // //destory 
    // const nonce = await mywallet.getTransactionCount();
    // const gasPrice = await provider.getGasPrice();
    // const gasLimit = 500000; // 根据具体情况设置适当的 gas limit

    // const unsignedTx = {
    //     to: '0x00d134bC5000a40028CF00004043B9004cD1b800',
    //     data: '0x38',
    //     nonce,
    //     gasPrice,
    //     gasLimit
    // };
    // const res2 = await (await mywallet.sendTransaction(unsignedTx)).wait();
    // console.log(res2);

}


main();


