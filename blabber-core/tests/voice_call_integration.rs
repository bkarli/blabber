use anyhow::Result;
use blabber_core::identity::Identity;
use blabber_core::node::Node;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn two_nodes_can_establish_a_voice_call() -> Result<()> {
    //Node A -> caller
    let identity_a = Identity::new("Alice");
    let mut node_a = Node::new(identity_a);
    node_a.create_endpoint().await?;
    node_a.run().await?;

    //Node B -> receiver
    let identity_b = Identity::new("Bob");
    let mut node_b = Node::new(identity_b);
    node_b.create_endpoint().await?;
    node_b.run().await?;

    let bob_endpoint_id = node_b.endpoint.as_ref().expect("endpoint not created").addr();

    let (_capture_a, _playback_a) = node_a.call(bob_endpoint_id).await?;
    sleep(Duration::from_millis(500)).await;

    println!("Call steht - sprich jetzt ins Mikrofon (5 Sekunden)");
    sleep(Duration::from_secs(5)).await;

    Ok(())
}