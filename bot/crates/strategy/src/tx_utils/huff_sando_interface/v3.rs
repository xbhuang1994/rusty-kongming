use cfmms::pool::UniswapV3Pool;
use eth_encode_packed::{SolidityDataType, TakeLastXBytes};
use ethers::{
    abi::{encode, Token},
    types::{Address, U256},
};

use crate::constants::WETH_ADDRESS;

use super::common::{
    five_byte_encoder::FiveByteMetaData, get_jump_dest_from_sig,
};

pub fn v3_create_frontrun_payload_multi(
    pool: UniswapV3Pool,
    output_token: Address,
    amount_in: U256,
) -> (Vec<u8>, U256) {
    let five_bytes = FiveByteMetaData::encode(amount_in, 2);
    let (payload, _) = eth_encode_packed::abi::encode_packed(&[
        SolidityDataType::NumberWithShift(
            get_jump_dest_from_sig(if *WETH_ADDRESS < output_token {
                "v3_frontrun0_multi"
            } else {
                "v3_frontrun1_multi"
            })
            .into(),
            TakeLastXBytes(8),
        ),
        SolidityDataType::Address(pool.address().0.into()),
        SolidityDataType::Bytes(&get_pool_key_hash(pool).to_vec()),
        SolidityDataType::Bytes(&five_bytes.finalize_to_bytes()),
        
    ]);

    let encoded_value = U256::zero();

    (payload, encoded_value)
}

pub fn v3_create_backrun_payload_multi(
    pool: UniswapV3Pool,
    input_token: Address,
    amount_in: U256,
) -> Vec<u8> {
    let five_bytes = FiveByteMetaData::encode(amount_in, 2);

    let (payload, _) = eth_encode_packed::abi::encode_packed(&[
        SolidityDataType::NumberWithShift(
            get_jump_dest_from_sig(if *WETH_ADDRESS < input_token {
                "v3_backrun0_multi"
            } else {
                "v3_backrun1_multi"
            })
            .into(),
            TakeLastXBytes(8),
        ),
        SolidityDataType::Address(pool.address().0.into()),
        SolidityDataType::Bytes(&get_pool_key_hash(pool).to_vec()),
        SolidityDataType::Bytes(&five_bytes.finalize_to_bytes()),
        SolidityDataType::Address(input_token.0.into()),
        
    ]);

    payload
}
/// https://github.com/Uniswap/v3-periphery/blob/6cce88e63e176af1ddb6cc56e029110289622317/contracts/libraries/PoolAddress.sol#L41C80-L41C80
fn get_pool_key_hash(pool: UniswapV3Pool) -> [u8; 32] {
    ethers::utils::keccak256(encode(&[
        Token::Address(pool.token_a),
        Token::Address(pool.token_b),
        Token::Uint(pool.fee.into()),
    ]))
}
