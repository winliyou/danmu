#[cfg(test)]
mod json_deser {
    use serde::Deserialize;
    #[derive(Deserialize)]
    struct MyStruct {
        name: String,
    }

    #[test]
    fn test_deserialize_partial_outter() {
        let json = r#"{"name": "John","age":100}"#;
        let my_struct: MyStruct = serde_json::from_str(json).unwrap();
        assert_eq!(my_struct.name, "John");
    }

    #[test]
    #[should_panic(expected = "json string must match partial in outer")]
    fn test_deserialize_partial_inner() -> () {
        let json = r#"{"age":100,"inner":{"name":"John","another_key":111}}"#;
        let my_struct: MyStruct =
            serde_json::from_str(json).expect("json string must match partial in outer");
        assert_eq!(my_struct.name, "John");
    }
}
