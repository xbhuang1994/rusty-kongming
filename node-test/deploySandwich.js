require('dotenv').config();
const ethers = require('ethers');
const shell = require('shelljs');
// const wethContractABI = require('./abi/IWETH.json');
const contractFactoryABI = require('./abi/IMetamorphicContractFactory.json');
// Create an ethers provider instance
const provider = new ethers.providers.JsonRpcProvider(process.env.RPC_URL_WSS);
const wallet = new ethers.Wallet(process.env.DEPOLY_KEY, provider);
// Create a contract instance
const contractAddress = '0x00000000e82eb0431756271F0d00CFB143685e7B'; // Replace with the actual contract address


async function main() {
    let balance = await wallet.getBalance();
    console.log(balance.toString());
    console.log("my wallet address:", wallet.address);
    const contractFactory = new ethers.Contract(contractAddress, contractFactoryABI, wallet);
    const slat = process.env.DEPOLY_SLAT;
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


