fn main() {
    // 测试不同 u16 值的 JSON 序列化长度
    let test_values = vec![0u16, 1, 10, 100, 999, 1000, 9999, 65535];
    
    for val in test_values {
        let json_bytes = serde_json::to_vec(&val).unwrap();
        println!("值: {:5} | JSON: {:?} | 长度: {} 字节", 
                 val, 
                 String::from_utf8_lossy(&json_bytes),
                 json_bytes.len());
    }
}
