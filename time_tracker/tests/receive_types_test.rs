//! Test to verify ReceiveTypes enum variants.

#[cfg(test)]
mod tests {
    use time_tracker::receive_types::ReceiveTypes;

    #[test]
    fn test_receive_types_variants() {
        // Test that all variants exist and can be created
        let _longest_session = ReceiveTypes::LongestSession;
        let _duration = ReceiveTypes::Duration;
        let _launches = ReceiveTypes::Launches;
        let _timeline = ReceiveTypes::Timeline;
        // Test that variants can be cloned
        let cloned = _duration.clone();
        match cloned {
            ReceiveTypes::Duration => assert!(true),
            _ => assert!(false, "Cloned variant is not Duration"),
        }

        // Test debug formatting
        let debug_output = format!("{:?}", _launches);
        assert_eq!(debug_output, "Launches");

        println!("All ReceiveTypes variants tested successfully");
    }
}