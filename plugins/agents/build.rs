fn main() {
    prost_build::Config::new()
        .compile_protos(
            &[
                "proto/arp/v1/types.proto",
                "proto/arp/v1/project.proto",
                "proto/arp/v1/workspace.proto",
                "proto/arp/v1/agent.proto",
                "proto/arp/v1/discovery.proto",
                "proto/arp/v1/token.proto",
            ],
            &["proto"],
        )
        .unwrap();
}
