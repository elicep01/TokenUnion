package main

import (
	"context"
	"fmt"
	"log"
	"os"
	"os/signal"
	"syscall"

	"github.com/libp2p/go-libp2p"
	"github.com/libp2p/go-libp2p/core/host"
	"github.com/libp2p/go-libp2p/core/peer"
	"github.com/libp2p/go-libp2p/p2p/protocol/circuitv2/relay"
)

func buildRelayHost(ctx context.Context, listenAddr string) (host.Host, error) {
	h, err := libp2p.New(
		libp2p.ListenAddrStrings(listenAddr),
	)
	if err != nil {
		return nil, err
	}

	_, err = relay.New(h)
	if err != nil {
		return nil, err
	}

	return h, nil
}

func main() {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	listenAddr := "/ip4/0.0.0.0/tcp/4001"
	if env := os.Getenv("RELAY_LISTEN_ADDR"); env != "" {
		listenAddr = env
	}

	h, err := buildRelayHost(ctx, listenAddr)
	if err != nil {
		log.Fatalf("failed to start relay host: %v", err)
	}
	defer h.Close()

	fmt.Println("TokenUnion relay started")
	fmt.Printf("PeerID: %s\n", h.ID().String())
	for _, addr := range h.Addrs() {
		fmt.Printf("Relay multiaddr: %s/p2p/%s\n", addr.String(), h.ID().String())
	}

	printExample(h.ID())

	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, os.Interrupt, syscall.SIGTERM)
	<-sigCh

	fmt.Println("shutting down relay...")
}

func printExample(relayID peer.ID) {
	fmt.Println("Use this relay addr in TokenUnion settings (relay_multiaddr):")
	fmt.Printf("/dns4/YOUR_RELAY_HOST/tcp/4001/p2p/%s\n", relayID.String())
}
