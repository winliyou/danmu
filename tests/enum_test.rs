#[cfg(test)]
mod enum_test {
    use serde::{Serialize, Serializer};
    #[test]
    fn test_enum() {
        #[derive(Debug, Serialize)]
        #[serde(untagged)]
        enum MyEnum {
            #[serde(serialize_with = "serialize_enum_as_int")]
            Var1(u32),
            #[serde(serialize_with = "serialize_enum_as_int")]
            Var2(u32),
        }

        fn serialize_enum_as_int<S>(value: &u32, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            serializer.serialize_i32(*value as i32)
        }

        #[derive(Debug, Serialize)]
        #[serde(untagged)]
        #[repr(u32)]
        enum MyEnum2 {
            #[serde(serialize_with = "serialize_enum_as_int")]
            Var1 = 1,
            Var2(String),
        }

        #[derive(Serialize)]
        pub struct MyEnunStruct {
            en_var: MyEnum,
        }
        #[derive(Serialize)]
        pub struct MyEnunStruct2 {
            en_var: MyEnum2,
        }
        let my_enun_struct = MyEnunStruct {
            en_var: MyEnum::Var1(123),
        };
        let mystr = serde_json::to_string(&my_enun_struct).unwrap();
        println!("en_var: {:?}", my_enun_struct.en_var);
        println!("serial result: {}", mystr);

        let my_enun_struct2 = MyEnunStruct2 {
            en_var: MyEnum2::Var1,
        };
        let mystr = serde_json::to_string(&my_enun_struct2).unwrap();
        println!("en_var: {:?}", my_enun_struct2.en_var);
        println!("serial result2: {}", mystr);
        let enum_var_1 = MyEnum::Var1(123);
        let enum_var_2 = MyEnum::Var2(456);
        println!("enum_var_1: {enum_var_1:?}, enum_var_2: {enum_var_2:?}");
    }
}
