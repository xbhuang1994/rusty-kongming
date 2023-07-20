use crate::{prelude::Pool, utils};

use super::*;

use hashbrown::HashMap;

#[derive(Debug, Clone)]
pub struct SandwichLogicV3 {
    jump_labels: HashMap<String, u32>,
}

impl SandwichLogicV3 {
    // Create a new `SandwichLogicV3` instance
    pub fn new() -> Self {
        let mut jump_labels: HashMap<String, u32> = HashMap::new();

        // encachement: turn this into a macro or constant?
        let jump_label_names = vec![
            "v3_output1_big",
            "v3_output0_big",
            "v3_output1_small",
            "v3_output0_small",
            "v3_input0",
            "v3_input1",
        ];

        let start_offset = 26;

        for x in 0..jump_label_names.len() {
            jump_labels.insert(
                jump_label_names[x].to_string(),
                start_offset + (5 * (x as u32)),
            );
        }
        jump_labels.insert("multi_call_v3_input0".to_string(), 81);
        jump_labels.insert("multi_call_v3_input1".to_string(), 86);
        jump_labels.insert("multi_call_v3_output0".to_string(), 91);
        jump_labels.insert("multi_call_v3_output1".to_string(), 96);

        SandwichLogicV3 { jump_labels }
    }

    // Handles creation of tx data field when weth is input
    pub fn create_payload_weth_is_input(
        &self,
        amount_in: I256,
        input: Address,
        output: Address,
        pool: Pool,
        block_number: U64,
    ) -> (Vec<u8>, U256) {
        let (token_0, token_1, fee) = (pool.token_0, pool.token_1, pool.swap_fee);
        let swap_type = self._find_swap_type(true, input, output, amount_in);
        let pool_key_hash = ethers::utils::keccak256(abi::encode(&[
            abi::Token::Address(token_0),
            abi::Token::Address(token_1),
            abi::Token::Uint(fee),
        ]));

        let (payload, _) = utils::encode_packed(&[
            utils::PackedToken::NumberWithShift(swap_type, utils::TakeLastXBytes(8)),
            utils::PackedToken::Address(pool.address),
            utils::PackedToken::Bytes(&pool_key_hash),
            utils::PackedToken::NumberWithShift(block_number.as_u64().into(), utils::TakeLastXBytes(32)),
        ]);

        let encoded_call_value = U256::from(amount_in.as_u128()) / get_weth_encode_divisor();

        (payload, encoded_call_value)
    }

    pub fn create_payload_weth_is_input_multi_call(
        &self,
        amount_in: U256,
        input: Address,
        output: Address,
        pool: Pool,
    ) -> (Vec<u8>, U256) {
        let (token_0, token_1, fee) = (pool.token_0, pool.token_1, pool.swap_fee);
        let swap_type = self._find_swap_type_multi(true, input, output);
        let pool_key_hash = ethers::utils::keccak256(abi::encode(&[
            abi::Token::Address(token_0),
            abi::Token::Address(token_1),
            abi::Token::Uint(fee),
        ]));
        let encoded_input_value = v2::encode_four_bytes(amount_in, false, false);
        let (payload, _) = utils::encode_packed(&[
            utils::PackedToken::NumberWithShift(swap_type, utils::TakeLastXBytes(8)),
            utils::PackedToken::Address(pool.address),
            utils::PackedToken::Bytes(&pool_key_hash),
            utils::PackedToken::NumberWithShift(
                encoded_input_value.mem_offset,
                utils::TakeLastXBytes(8),
            ),
            utils::PackedToken::NumberWithShift(
                encoded_input_value.four_byte_value,
                utils::TakeLastXBytes(32),
            ),
        ]);
        (payload, U256::zero())
    }

    // Handles creation of tx data field when weth is output
    pub fn create_payload_weth_is_output(
        &self,
        amount_in: I256,
        input: Address,
        output: Address,
        pool: Pool,
    ) -> Vec<u8> {
        let (token_0, token_1, fee) = (pool.token_0, pool.token_1, pool.swap_fee);
        let swap_type = self._find_swap_type(false, input, output, amount_in);
        let pool_key_hash = ethers::utils::keccak256(abi::encode(&[
            abi::Token::Address(token_0),
            abi::Token::Address(token_1),
            abi::Token::Uint(fee),
        ]));

        let payload;

        if amount_in <= I256::from(281474976710655u128) {
            // use small encoding method (encode amount_in to 6 bytes)
            (payload, _) = utils::encode_packed(&vec![
                utils::PackedToken::NumberWithShift(swap_type, utils::TakeLastXBytes(8)),
                utils::PackedToken::Address(pool.address),
                utils::PackedToken::Address(input),
                utils::PackedToken::NumberWithShift(
                    amount_in.as_u128().into(),
                    utils::TakeLastXBytes(48),
                ),
                utils::PackedToken::Bytes(&pool_key_hash),
            ]);
        } else {
            // use big encoding method (encode amount_in by dividing by 1e13 and storing result into 9 bytes)
            let encoded_amount_in = amount_in / I256::from_dec_str("10000000000000").unwrap();
            (payload, _) = utils::encode_packed(&vec![
                utils::PackedToken::NumberWithShift(swap_type, utils::TakeLastXBytes(8)),
                utils::PackedToken::Address(pool.address),
                utils::PackedToken::Address(input),
                utils::PackedToken::NumberWithShift(
                    encoded_amount_in.as_u128().into(),
                    utils::TakeLastXBytes(72),
                ),
                utils::PackedToken::Bytes(&pool_key_hash),
            ]);
        }

        payload
    }

     // Handles creation of tx data field when weth is output
     pub fn create_payload_weth_is_output_multi_call(
        &self,
        amount_in: U256,
        input: Address,
        output: Address,
        pool: Pool,
    ) -> Vec<u8> {
        let (token_0, token_1, fee) = (pool.token_0, pool.token_1, pool.swap_fee);
        let swap_type = self._find_swap_type_multi(false, input, output);
        let pool_key_hash = ethers::utils::keccak256(abi::encode(&[
            abi::Token::Address(token_0),
            abi::Token::Address(token_1),
            abi::Token::Uint(fee),
        ]));

        let payload;
        let encoded_input_value = v2::encode_four_bytes(amount_in, false, false);
        // use big encoding method (encode amount_in by dividing by 1e13 and storing result into 9 bytes)
        // let encoded_amount_in = amount_in / I256::from_dec_str("10000000000000").unwrap();
        (payload, _) = utils::encode_packed(&vec![
            utils::PackedToken::NumberWithShift(swap_type, utils::TakeLastXBytes(8)),
            utils::PackedToken::Address(pool.address),
            utils::PackedToken::Bytes(&pool_key_hash),
            utils::PackedToken::NumberWithShift(
                encoded_input_value.mem_offset,
                utils::TakeLastXBytes(8),
            ),
            utils::PackedToken::NumberWithShift(
                encoded_input_value.four_byte_value,
                utils::TakeLastXBytes(32),
            ),
            utils::PackedToken::Address(input),
        ]);

        payload
    }
    // Internal helper function to find correct JUMPDEST
    fn _find_swap_type(
        &self,
        is_weth_input: bool,
        input: Address,
        output: Address,
        amount_in: I256,
    ) -> U256 {
        let swap_type: u32 = match (
            is_weth_input,
            (input < output),
            (amount_in <= I256::from(281474976710655u128)), // 281474976710655 (0xFFFFFFFFFFFF)
        ) {
            // weth is input and token0
            (true, true, _) => self.jump_labels["v3_input0"],
            // weth is input and token1
            (true, false, _) => self.jump_labels["v3_input1"],
            // weth is output and token1 && amountIn <= 281474976710655
            (false, true, true) => self.jump_labels["v3_output1_small"],
            // weth is output and token1 && amountIn > 281474976710655
            (false, true, false) => self.jump_labels["v3_output1_big"],
            // weth is output and token0 && amountIn <= 281474976710655
            (false, false, true) => self.jump_labels["v3_output0_small"],
            // weth is output and token0 && amountIn > 281474976710655
            (false, false, false) => self.jump_labels["v3_output0_big"],
        };

        U256::from(swap_type)
    }

    // Internal helper function to find correct JUMPDEST multi call
    fn _find_swap_type_multi(&self, is_weth_input: bool, input: Address, output: Address) -> U256 {
        let swap_type: u32 = match (is_weth_input, (input < output)) {
            // weth is input and token0
            (true, true) => self.jump_labels["multi_call_v3_input0"],
            // weth is input and token1
            (true, false) => self.jump_labels["multi_call_v3_input1"],
            // weth is output and token1
            (false, true) => self.jump_labels["multi_call_v3_output1"],
            // weth is output and token0
            (false, false) => self.jump_labels["multi_call_v3_output0"],
        };

        U256::from(swap_type)
    }
}

/// returns the encoded value of amount in (actual value passed to contract)
pub fn encode_intermediary_token(amount_in: U256) -> U256 {
    (amount_in / U256::from(10000000000000u128)) * U256::from(10000000000000u128)
}

/// returns the encoded value of amount in (actual value passed to contract)
pub fn encode_weth(amount_in: U256) -> U256 {
    (amount_in / get_weth_encode_divisor()) * get_weth_encode_divisor()
}
