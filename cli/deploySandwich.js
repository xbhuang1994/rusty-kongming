require('dotenv').config();
const ethers = require('ethers');
const shell = require('shelljs');
// const wethContractABI = require('./abi/IWETH.json');
const contractFactoryABI = require('./abi/IMetamorphicContractFactory.json');
// Create an ethers provider instance
const provider = new ethers.providers.JsonRpcProvider(process.env.RPC_URL);
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

}


main();


