use cfmms::pool::UniswapV2Pool;
use eth_encode_packed::{SolidityDataType, TakeLastXBytes};
use ethers::types::{Address, U256};

use crate::constants::WETH_ADDRESS;

use super::common::{
    five_byte_encoder::FiveByteMetaData, get_jump_dest_from_sig,
};

// pub fn v2_create_frontrun_payload(
//     pool: UniswapV2Pool,
//     output_token: Address,
//     amount_in: U256,
//     amount_out: U256, // amount_out is needed to be passed due to taxed tokens
//     target_block_number:U64
// ) -> (Vec<u8>, U256) {
//     let jump_dest = get_jump_dest_from_sig(if *WETH_ADDRESS < output_token {
//         "v2_frontrun0"
//     } else {
//         "v2_frontrun1"
//     });

//     let five_bytes =
//         FiveByteMetaData::encode(amount_out, if *WETH_ADDRESS < output_token { 1 } else { 0 });

//     let (payload, _) = eth_encode_packed::abi::encode_packed(&[
//         SolidityDataType::NumberWithShift(jump_dest.into(), TakeLastXBytes(8)),
//         SolidityDataType::Address(pool.address().0.into()),
//         SolidityDataType::Bytes(&five_bytes.finalize_to_bytes()),
//         SolidityDataType::NumberWithShift(target_block_number.as_u64().into(), TakeLastXBytes(32)),
//     ]);

//     let encoded_call_value = WethEncoder::encode(amount_in);

//     (payload, encoded_call_value)
// }

// /// dev: amount_out is needed to be passed due to taxed tokens
// pub fn v2_create_backrun_payload(
//     pool: UniswapV2Pool,
//     input_token: Address,
//     amount_in: U256,
//     amount_out: U256, // amount_out is needed to be passed due to taxed tokens
// ) -> (Vec<u8>, U256) {
//     let jump_dest = get_jump_dest_from_sig(if *WETH_ADDRESS < input_token {
//         "v2_backrun0"
//     } else {
//         "v2_backrun1"
//     });

//     let five_bytes = FiveByteMetaData::encode(amount_in, 1);

//     let (payload, _) = eth_encode_packed::abi::encode_packed(&[
//         SolidityDataType::NumberWithShift(jump_dest.into(), TakeLastXBytes(8)),
//         SolidityDataType::Address(pool.address().0.into()),
//         SolidityDataType::Address(input_token.0.into()),
//         SolidityDataType::Bytes(&five_bytes.finalize_to_bytes()),
//     ]);

//     let encoded_call_value = WethEncoder::encode(amount_out);

//     (payload, encoded_call_value)
// }


pub fn v2_create_frontrun_payload_multi(
    pool: UniswapV2Pool,
    output_token: Address,
    amount_in: U256,
    amount_out: U256, // amount_out is needed to be passed due to taxed tokens
) -> (Vec<u8>, U256) {
    let jump_dest = get_jump_dest_from_sig("v2_frontrun_multi");

    let five_bytes_input = FiveByteMetaData::encode(amount_in, 0);
    let five_bytes_output = FiveByteMetaData::encode(amount_out, if *WETH_ADDRESS < output_token {1} else {0});

    let (payload, _) = eth_encode_packed::abi::encode_packed(&[
        SolidityDataType::NumberWithShift(jump_dest.into(), TakeLastXBytes(8)),
        SolidityDataType::Address(pool.address().0.into()),
        SolidityDataType::Bytes(&five_bytes_input.finalize_to_bytes()),
        SolidityDataType::Bytes(&five_bytes_output.finalize_to_bytes()),
        
    ]);

    let encoded_call_value = U256::zero();

    (payload, encoded_call_value)
}
/// dev: amount_out is needed to be passed due to taxed tokens
pub fn v2_create_backrun_payload_multi(
    pool: UniswapV2Pool,
    input_token: Address,
    amount_in: U256,
    amount_out: U256, // amount_out is needed to be passed due to taxed tokens
) -> (Vec<u8>, U256) {
    let jump_dest = get_jump_dest_from_sig("v2_backrun_multi");

    let five_bytes_input = FiveByteMetaData::encode(amount_in, 1);
    let five_bytes_output = FiveByteMetaData::encode(amount_out, if *WETH_ADDRESS < input_token {0} else {1});

    let (payload, _) = eth_encode_packed::abi::encode_packed(&[
        SolidityDataType::NumberWithShift(jump_dest.into(), TakeLastXBytes(8)),
        SolidityDataType::Address(pool.address().0.into()),
        SolidityDataType::Address(input_token.0.into()),
        SolidityDataType::Bytes(&five_bytes_input.finalize_to_bytes()),
        SolidityDataType::Bytes(&five_bytes_output.finalize_to_bytes()),
    ]);

    let encoded_call_value = U256::zero();

    (payload, encoded_call_value)
}