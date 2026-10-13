//! WASM bindings for the swipe engine (feature-gated)

use crate::SwipeEngine;
use std::cell::RefCell;
use wasm_bindgen::prelude::*;

thread_local! {
    static ENGINE: RefCell<SwipeEngine> = RefCell::new(SwipeEngine::new());
}

#[wasm_bindgen]
pub fn init_dictionary(freq_text: &str) {
    ENGINE.with(|e| e.borrow_mut().load_dictionary(freq_text));
}

#[wasm_bindgen]
pub fn set_pop_weight(weight: f64) {
    ENGINE.with(|e| e.borrow_mut().set_pop_weight(weight));
}

#[wasm_bindgen]
pub fn predict_wasm(swipe_input: &str, limit: usize) -> String {
    ENGINE.with(|e| {
        let predictions = e.borrow().predict(swipe_input, limit);
        serde_json::to_string(&predictions).unwrap_or_else(|_| "[]".to_string())
    })
}
