use eth_encode_packed::{SolidityDataType, TakeLastXBytes};
use ethers::types::U64;

/// This Module file holds common methods used for both v2 and v3 methods

/// Utils to encode (and decode) 32 bytes to 5 bytes of calldata
pub mod five_byte_encoder;

/// Utils to encode (and decode) weth to `tx.value`
pub mod weth_encoder;

// Declare the array as static
static FUNCTION_NAMES: [&str; 18] = [
    "v2_backrun0",
    "v2_frontrun0",
    "v2_backrun1",
    "v2_frontrun1",
    "v3_backrun0",
    "v3_frontrun0",
    "v3_backrun1",
    "v3_frontrun1",
    "seppuku",
    "recoverEth",
    "recoverWeth",
    "v2_backrun_multi",
    "v2_frontrun_multi",
    "v3_backrun0_multi",
    "v3_frontrun0_multi",
    "v3_backrun1_multi",
    "v3_frontrun1_multi",
    "check_block_number"
];

pub fn get_jump_dest_from_sig(function_name: &str) -> u8 {
    let starting_index = 0x05;

    // find index of associated JUMPDEST (sig)
    for (i, &name) in FUNCTION_NAMES.iter().enumerate() {
        if name == function_name {
            return (i as u8 * 5) + starting_index;
        }
    }

    // not found (force jump to invalid JUMPDEST)
    0x00
}
//limit block height to next block height
pub fn limit_block_height(call_data:Vec<u8> ,next_block_number: U64) -> Vec<u8> {
    let (payload,_) = eth_encode_packed::abi::encode_packed(&[
        SolidityDataType::NumberWithShift(get_jump_dest_from_sig("check_block_number").into(), TakeLastXBytes(8)),
        SolidityDataType::NumberWithShift(next_block_number.as_u64().into(), TakeLastXBytes(32)),
        SolidityDataType::Bytes(call_data.as_slice()),
    ]);
    payload
}
