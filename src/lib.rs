pub mod adapter;
pub mod diary;
pub mod finding;
pub mod inventory;
pub mod model;
pub mod rules;
pub mod store;

#[cfg(test)]
mod smoke {
    #[test]
    fn crate_compiles() {
        assert_eq!(2 + 2, 4);
    }
}
