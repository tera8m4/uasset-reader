#[derive(Debug, Clone)]
pub struct FName {
    pub index: i32,
    pub number: i32,
}

impl FName {
    pub fn is_none(&self) -> bool {
        self.index == 0
    }
}