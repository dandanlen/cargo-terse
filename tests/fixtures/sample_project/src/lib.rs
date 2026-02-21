pub fn add(a: i32, b: i32) -> i32 {
    let result = a + b;
    let extra = 42;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        assert_eq!(add(2, 3), 5);
    }
}
