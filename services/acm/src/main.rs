fn main() {
    println!("Hello, world!");
}

#[cfg(test)]
mod tests {

    #[test]
    fn it_works() {
        assert_eq!(4, 2 + 2)
    }

    #[test]
    fn it_really_works() {
        assert_eq!(4, 2 + 2)
    }

    #[test]
    fn it_really_super_works() {
        assert_eq!(4, 2 + 2)
    }

    #[test]
    fn it_really_super_works_well() {
        assert_eq!(4, 2 + 2)
    }
}
