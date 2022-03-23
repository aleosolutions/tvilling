use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct StartRequest {
    pub count: u32,
}

mod tests {
    use super::*;

    #[test]
    fn start_request_deserializable_from_json() {
        let json_msg = r#"
            {
                "count": 5
            }
        "#;

        let _request: StartRequest = serde_json::from_str(json_msg).unwrap();
    }

    #[test]
    #[should_panic]
    fn negative_count_doesnt_deserialize() {
        let json_msg = r#"
            {
                "count": -5
            }
        "#;

        let _request: StartRequest = serde_json::from_str(json_msg).unwrap();
    }
}
