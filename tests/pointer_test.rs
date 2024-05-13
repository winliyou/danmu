#[cfg(test)]
mod pointer_test {

    #[test]
    fn test_pointer() {
        let mut a = 1;
        println!("a ptr: {:p}", &a);
        let b = &mut a;
        *b = 123;
        println!("b: {:?}", b);
        println!("a ptr: {:p}", &a);
        println!("a: {}", a);
    }
}
