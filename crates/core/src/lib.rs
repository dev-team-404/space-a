pub mod adapter;
pub mod chat;
pub mod coach;
pub mod curation;
pub mod diary;
pub mod finding;
pub mod hosts;
pub mod inventory;
pub mod mascot;
pub mod model;
pub mod ops;
pub mod pipeline;
pub mod rules;
pub mod store;
pub mod transcript;

#[cfg(test)]
mod smoke {
    #[test]
    fn crate_compiles() {
        assert_eq!(2 + 2, 4);
    }
}
