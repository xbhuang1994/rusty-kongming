require('dotenv').config();
const ethers = require('ethers');
const wethContractABI = require('./abi/IWETH.json');
// Create an ethers provider instance
const provider = new ethers.providers.JsonRpcProvider(process.env.RPC_URL);
const wallet = new ethers.Wallet(process.env.DEPOLY_KEY, provider);
const wethAddress = '0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2';
async function main() {
    // 使用合约地址和 ABI 创建 WETH 合约实例
    const wethContract = new ethers.Contract(wethAddress, wethContractABI, wallet);
    const wethBalance = await wethContract.balanceOf(process.env.SANDWICH_CONTRACT);
    const nonce = await wallet.getTransactionCount();
    const gasPrice = await provider.getGasPrice();
    const gasLimit = 500000; // 根据具体情况设置适当的 gas limit
    
    // Craft our payload
    const payload = ethers.utils.solidityPack(
        ["uint8", "uint256"],
        [
            66,
            wethBalance
        ]
    );
    console.log(payload);
    const unsignedTx = {
        to: process.env.SANDWICH_CONTRACT,
        data: payload,
        nonce,
        gasPrice,
        gasLimit
    };
    const res = await (await wallet.sendTransaction(unsignedTx)).wait();
    console.log(res);
}


main();


