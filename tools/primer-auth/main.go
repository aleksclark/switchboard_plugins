package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"io"
	"log"
	"log/slog"
	"net"
	"net/http"
	"os"
	"os/exec"
	"os/signal"
	"strconv"
	"syscall"
	"time"
)

func main() {
	os.Exit(run(os.Args[1:], os.Stderr))
}

func run(args []string, output io.Writer) int {
	flags := flag.NewFlagSet("primer-auth", flag.ContinueOnError)
	flags.SetOutput(output)
	configPath := flags.String("config", "", "Path to a private JSON configuration file")
	if err := flags.Parse(args); err != nil {
		if errors.Is(err, flag.ErrHelp) {
			return 0
		}
		return 2
	}
	if flags.NArg() != 0 {
		fmt.Fprintln(output, "unexpected positional arguments")
		return 2
	}
	cfg, err := loadConfig(*configPath)
	if err != nil {
		fmt.Fprintln(output, err)
		return 1
	}
	command, err := exec.LookPath(cfg.BrowserCommand)
	if err != nil {
		fmt.Fprintln(output, "browser executable unavailable")
		return 1
	}
	cfg.BrowserCommand = command
	logger := slog.New(slog.NewJSONHandler(output, nil))
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	if err := serve(ctx, cfg, logger); err != nil {
		logger.Error("companion_stopped", "reason", "listener_or_server_failure")
		return 1
	}
	return 0
}

func serve(ctx context.Context, cfg config, logger *slog.Logger) error {
	address := "127.0.0.1:" + strconv.Itoa(cfg.Port)
	listener, err := net.Listen("tcp4", address)
	if err != nil {
		return errors.New("listener unavailable")
	}
	defer listener.Close()
	handler := newCompanion(cfg, &commandRunner{cfg: cfg}, logger)
	server := &http.Server{
		Handler:           handler,
		ReadHeaderTimeout: 3 * time.Second,
		ReadTimeout:       5 * time.Second,
		WriteTimeout:      35 * time.Second,
		IdleTimeout:       30 * time.Second,
		MaxHeaderBytes:    16 * 1024,
		ErrorLog:          log.New(io.Discard, "", 0),
		BaseContext:       func(net.Listener) context.Context { return ctx },
	}
	done := make(chan error, 1)
	go func() { done <- server.Serve(listener) }()
	logger.Info("companion_started", "address", address)
	select {
	case err := <-done:
		if !errors.Is(err, http.ErrServerClosed) {
			return errors.New("server unavailable")
		}
	case <-ctx.Done():
		shutdown, cancel := context.WithTimeout(context.Background(), time.Second)
		defer cancel()
		if server.Shutdown(shutdown) != nil {
			_ = server.Close()
		}
		<-done
	}
	logger.Info("companion_stopped")
	return nil
}
