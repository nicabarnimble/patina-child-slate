#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

wit_bindgen::generate!({
    path: "wit",
    world: "slate-manager",
    generate_all,
});

mod control;
mod dependency_graph;
mod dispatch;
mod lifecycle;
mod model;
mod narrative;
mod runtime;
mod slate_body;
mod spec_bridge;
mod store;
mod text;
mod work_commands;
mod work_fields;
mod work_views;

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
struct SlateManager;

#[cfg(test)]
mod slate_native_tests;

#[cfg(target_arch = "wasm32")]
export!(SlateManager);
