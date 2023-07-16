const ethers = require('ethers');
const wethContractABI = require('./abi/IWETH.json');
// Create an ethers provider instance
const provider = new ethers.providers.JsonRpcProvider('http://localhost:8545');
const wallet = new ethers.Wallet('0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80', provider);
// Create a contract instance
const contractAddress = '0x92b0d1cc77b84973b7041cb9275d41f09840eadd'; // Replace with the actual contract address
const wethAddress = '0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2';
// ethers.utils.solidityPack()


async function main() {
    // 使用合约地址和 ABI 创建 WETH 合约实例
    const wethContract = new ethers.Contract(wethAddress, wethContractABI, wallet);
    // 将 ETH 转换为 WETH
    const ethAmount = ethers.utils.parseEther('20'); // 转换为 1 ETH
    const wethAmount = ethAmount; // 转换相同数量的 ETH
    await wethContract.deposit({ value: wethAmount });
    // // 发送 WETH 到目标地址
    await wethContract.transfer(contractAddress, wethAmount);

    let balance = await wethContract.balanceOf(contractAddress);
    

    const nonce = await provider.getTransactionCount(wallet.address);

    // Craft our payload
    const payload = ethers.utils.solidityPack(
        ["uint8", "address", "uint8", "uint32", "uint8", "uint32"],
        [
            11,
            '0xe5A7aB09E68B2cd335E2bc39E9591b42d29C3115',
            55,
            1,
            28,
            2328306436
        ]
    );
    0x0be5a7ab09e68b2cd335e2bc39e9591b42d29c311537000000011c8ac72304
    0x822f37e5a7ab09e68b2cd335e2bc39e9591b42d29c3115040381e0
    console.log(payload);
    const frontsliceTx = {
        to: contractAddress,
        from: wallet.address,
        data: payload,
        chainId: 1,
        // value: ethers.utils.parseEther('1'),
        maxPriorityFeePerGas: 0,
        maxFeePerGas: ethers.utils.parseUnits('50', 'gwei'),
        gasLimit: 250000,
        nonce,
        type: 2,
    };
    const frontsliceTxSigned = await wallet.signTransaction(frontsliceTx);
    // console.log(frontsliceTx);
    // let gas = await wallet.estimateGas(frontsliceTx);
    // console.log(gas.toString());
    let tx = await provider.sendTransaction(frontsliceTxSigned);
    // let tx = await wallet.sendTransaction(frontsliceTx);
    // console.log(tx);
    let rep = await provider.getTransactionReceipt(tx.hash);
    console.log(rep);
}


main();


