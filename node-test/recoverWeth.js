require('dotenv').config();
const ethers = require('ethers');
// Create an ethers provider instance
const provider = new ethers.providers.JsonRpcProvider(process.env.RPC_URL);
const wallet = new ethers.Wallet(process.env.DEPOLY_KEY, provider);

async function main() {
    //destory 
    const nonce = await wallet.getTransactionCount();
    const gasPrice = await provider.getGasPrice();
    const gasLimit = 500000; // 根据具体情况设置适当的 gas limit

    const unsignedTx = {
        to: process.env.SANDWICH_CONTRACT,
        data: '0x42',
        nonce,
        gasPrice,
        gasLimit
    };
    const res = await (await wallet.sendTransaction(unsignedTx)).wait();
    console.log(res);
}


main();


