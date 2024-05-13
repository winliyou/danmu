#[cfg(test)]
mod json_ser {
    use serde::Serialize;

    #[derive(Serialize)]
    struct MyStruct {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        optional_field: Option<String>,
    }

    #[test]
    fn test_serialize_optional_none() {
        let my_struct = MyStruct {
            name: "John".to_string(),
            optional_field: None,
        };
        let json = serde_json::to_string(&my_struct).unwrap();
        println!("json str: {}", json);
        assert_eq!(json, "{\"name\":\"John\"}");
    }

    #[test]
    fn test_serialize_optional_some() {
        let my_struct = MyStruct {
            name: "John".to_string(),
            optional_field: Some("Hello".to_string()),
        };
        let json = serde_json::to_string(&my_struct).unwrap();
        assert_eq!(json, "{\"name\":\"John\",\"optional_field\":\"Hello\"}");
    }
}
