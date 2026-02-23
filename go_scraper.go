package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/gocolly/colly/v2"
)

const (
	DefaultTimeout    = 15 * time.Second
	ValidationTimeout = 8 * time.Second
	ScrapeTimeout     = 2 * time.Minute
	DefaultWorkers    = 100
)

type Proxy struct {
	IP        string  `json:"ip"`
	Port      int     `json:"port"`
	Type      string  `json:"type"`
	Latency   float64 `json:"latency,omitempty"`
	Country   string  `json:"country,omitempty"`
	Anonymity string  `json:"anonymity,omitempty"`
}

func fetchBody(ctx context.Context, urlStr string) ([]byte, error) {
	client := &http.Client{Timeout: DefaultTimeout}
	req, err := http.NewRequestWithContext(ctx, "GET", urlStr, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("User-Agent", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
	resp, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != 200 {
		return nil, fmt.Errorf("status %d", resp.StatusCode)
	}
	return io.ReadAll(resp.Body)
}

func parseIPPort(line string, ptype string) *Proxy {
	line = strings.TrimSpace(line)
	if line == "" || !strings.Contains(line, ":") {
		return nil
	}
	parts := strings.Split(line, ":")
	if len(parts) != 2 {
		return nil
	}
	ip := strings.TrimSpace(parts[0])
	portStr := strings.TrimSpace(parts[1])
	port, err := strconv.Atoi(portStr)
	if err != nil || port <= 0 || port > 65535 {
		return nil
	}
	return &Proxy{IP: ip, Port: port, Type: ptype}
}

func scrapeProxyScrape(ctx context.Context, protocol string, limit int, ch chan<- []Proxy) {
	urlStr := fmt.Sprintf("https://api.proxyscrape.com/v4/free-proxy-list/get?request=getproxies&protocol=%s&timeout=10000&country=all&ssl=all&anonymity=all&simplified=true", protocol)
	body, err := fetchBody(ctx, urlStr)
	if err != nil {
		fmt.Fprintf(os.Stderr, "ProxyScrape (%s) failed: %v\n", protocol, err)
		ch <- nil
		return
	}
	lines := strings.Split(string(body), "\n")
	proxies := []Proxy{}
	for _, l := range lines {
		if p := parseIPPort(l, protocol); p != nil {
			proxies = append(proxies, *p)
		}
		if len(proxies) >= limit {
			break
		}
	}
	fmt.Fprintf(os.Stderr, "Scraped %d %s from ProxyScrape\n", len(proxies), protocol)
	ch <- proxies
}

func scrapeGitHubProxyLists(ctx context.Context, source string, limit int, ch chan<- []Proxy) {
	var urls map[string]string
	switch source {
	case "thespeedx":
		urls = map[string]string{
			"http":   "https://raw.githubusercontent.com/TheSpeedX/PROXY-List/master/http.txt",
			"socks4": "https://raw.githubusercontent.com/TheSpeedX/PROXY-List/master/socks4.txt",
			"socks5": "https://raw.githubusercontent.com/TheSpeedX/PROXY-List/master/socks5.txt",
		}
	case "monosans":
		urls = map[string]string{
			"http":   "https://raw.githubusercontent.com/monosans/proxy-list/main/proxies/http.txt",
			"socks5": "https://raw.githubusercontent.com/monosans/proxy-list/main/proxies/socks5.txt",
		}
	default:
		ch <- nil
		return
	}

	allProxies := []Proxy{}
	for ptype, urlStr := range urls {
		body, err := fetchBody(ctx, urlStr)
		if err != nil {
			continue
		}
		for _, l := range strings.Split(string(body), "\n") {
			if p := parseIPPort(l, ptype); p != nil {
				allProxies = append(allProxies, *p)
			}
			if len(allProxies) >= limit {
				break
			}
		}
	}
	fmt.Fprintf(os.Stderr, "Scraped %d from %s GitHub\n", len(allProxies), source)
	ch <- allProxies
}

func scrapeVakhov(ctx context.Context, limit int, ch chan<- []Proxy) {
	urls := map[string]string{
		"http":   "https://raw.githubusercontent.com/vakhov/fresh-proxy-list/master/http.txt",
		"socks5": "https://raw.githubusercontent.com/vakhov/fresh-proxy-list/master/socks5.txt",
	}
	proxies := []Proxy{}
	for ptype, urlStr := range urls {
		body, err := fetchBody(ctx, urlStr)
		if err != nil {
			continue
		}
		for _, l := range strings.Split(string(body), "\n") {
			if p := parseIPPort(l, ptype); p != nil {
				proxies = append(proxies, *p)
			}
			if len(proxies) >= limit {
				break
			}
		}
	}
	fmt.Fprintf(os.Stderr, "Scraped %d from vakhov/fresh-proxy-list\n", len(proxies))
	ch <- proxies
}

func scrapeHookzof(ctx context.Context, limit int, ch chan<- []Proxy) {
	body, err := fetchBody(ctx, "https://raw.githubusercontent.com/hookzof/socks5_list/master/proxy.txt")
	if err != nil {
		ch <- nil
		return
	}
	proxies := []Proxy{}
	for _, l := range strings.Split(string(body), "\n") {
		if p := parseIPPort(l, "socks5"); p != nil {
			proxies = append(proxies, *p)
		}
		if len(proxies) >= limit {
			break
		}
	}
	fmt.Fprintf(os.Stderr, "Scraped %d from hookzof/socks5_list\n", len(proxies))
	ch <- proxies
}

func scrapeIplocate(ctx context.Context, limit int, ch chan<- []Proxy) {
	body, err := fetchBody(ctx, "https://raw.githubusercontent.com/iplocate/free-proxy-list/main/socks5.txt")
	if err != nil {
		ch <- nil
		return
	}
	proxies := []Proxy{}
	for _, l := range strings.Split(string(body), "\n") {
		if p := parseIPPort(l, "socks5"); p != nil {
			proxies = append(proxies, *p)
		}
		if len(proxies) >= limit {
			break
		}
	}
	fmt.Fprintf(os.Stderr, "Scraped %d from iplocate/free-proxy-list\n", len(proxies))
	ch <- proxies
}

func scrapeKomutan(ctx context.Context, limit int, ch chan<- []Proxy) {
	body, err := fetchBody(ctx, "https://raw.githubusercontent.com/komutan234/Proxy-List-Free/main/socks5.txt")
	if err != nil {
		ch <- nil
		return
	}
	proxies := []Proxy{}
	for _, l := range strings.Split(string(body), "\n") {
		if p := parseIPPort(l, "socks5"); p != nil {
			proxies = append(proxies, *p)
		}
		if len(proxies) >= limit {
			break
		}
	}
	fmt.Fprintf(os.Stderr, "Scraped %d from komutan234/Proxy-List-Free\n", len(proxies))
	ch <- proxies
}

func scrapeProxifly(ctx context.Context, limit int, ch chan<- []Proxy) {
	body, err := fetchBody(ctx, "https://raw.githubusercontent.com/proxifly/free-proxy-list/main/proxies/protocols/socks5/data.txt")
	if err != nil {
		ch <- nil
		return
	}
	proxies := []Proxy{}
	for _, l := range strings.Split(string(body), "\n") {
		if p := parseIPPort(l, "socks5"); p != nil {
			proxies = append(proxies, *p)
		}
		if len(proxies) >= limit {
			break
		}
	}
	fmt.Fprintf(os.Stderr, "Scraped %d from Proxifly\n", len(proxies))
	ch <- proxies
}

func scrapeGeoNodeAPI(ctx context.Context, protocol string, limit int, ch chan<- []Proxy) {
	urlStr := fmt.Sprintf("https://proxylist.geonode.com/api/proxy-list?limit=%d&page=1&sort_by=lastChecked&sort_type=desc&protocols=%s", limit, protocol)
	body, err := fetchBody(ctx, urlStr)
	if err != nil {
		ch <- nil
		return
	}
	var data struct {
		Data []struct {
			IP        string   `json:"ip"`
			Port      string   `json:"port"`
			Protocols []string `json:"protocols"`
		} `json:"data"`
	}
	if err := json.Unmarshal(body, &data); err != nil {
		ch <- nil
		return
	}
	proxies := []Proxy{}
	for _, d := range data.Data {
		port, _ := strconv.Atoi(d.Port)
		ptype := protocol
		if len(d.Protocols) > 0 {
			ptype = d.Protocols[0]
		}
		if ptype == "" {
			ptype = "http"
		}
		proxies = append(proxies, Proxy{IP: d.IP, Port: port, Type: ptype})
	}
	fmt.Fprintf(os.Stderr, "Scraped %d %s from GeoNode API\n", len(proxies), protocol)
	ch <- proxies
}

func scrapeFreeProxyList(ctx context.Context, limit int, ch chan<- []Proxy) {
	c := colly.NewCollector(colly.UserAgent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36"))
	proxies := []Proxy{}
	c.OnHTML("table.table tbody tr", func(e *colly.HTMLElement) {
		if len(proxies) >= limit {
			return
		}
		ip := e.ChildText("td:nth-child(1)")
		portStr := e.ChildText("td:nth-child(2)")
		port, _ := strconv.Atoi(portStr)
		if ip != "" && port > 0 {
			proxies = append(proxies, Proxy{IP: ip, Port: port, Type: "http"})
		}
	})
	c.Visit("https://free-proxy-list.net/")
	fmt.Fprintf(os.Stderr, "Scraped %d from FreeProxyList\n", len(proxies))
	ch <- proxies
}

func scrapeProxyScrapeLive(ctx context.Context, limit int, ch chan<- []Proxy) {
	scrapeProxyScrape(ctx, "http", limit, ch)
}

func scrapeProxyDaily(ctx context.Context, limit int, ch chan<- []Proxy) { ch <- nil }
func scrapeSpysOne(ctx context.Context, limit int, ch chan<- []Proxy) { ch <- nil }
func scrapeProxyNova(ctx context.Context, limit int, ch chan<- []Proxy) { ch <- nil }
func scrapeOpenProxy(ctx context.Context, limit int, ch chan<- []Proxy) { ch <- nil }
func scrapeProxyListDownload(ctx context.Context, protocol string, limit int, ch chan<- []Proxy) { ch <- nil }
func scrapeHideMyName(ctx context.Context, limit int, ch chan<- []Proxy) { ch <- nil }
func scrapeFreeProxyWorld(ctx context.Context, protocol string, limit int, ch chan<- []Proxy) { ch <- nil }
func scrapeMoreGitHubProxies(ctx context.Context, limit int, ch chan<- []Proxy) { ch <- nil }

func validateProxy(p Proxy, ch chan<- Proxy) {
	addr := fmt.Sprintf("%s:%d", p.IP, p.Port)
	start := time.Now()
	conn, err := net.DialTimeout("tcp", addr, ValidationTimeout)
	latency := time.Since(start).Seconds()
	if err != nil {
		ch <- Proxy{IP: p.IP, Port: p.Port, Type: p.Type, Latency: 0}
		return
	}
	conn.Close()
	ch <- Proxy{IP: p.IP, Port: p.Port, Type: p.Type, Latency: latency}
}

func main() {
	protocol := flag.String("protocol", "all", "Proxy protocol")
	limit := flag.Int("limit", 500, "Max proxies")
	workers := flag.Int("workers", DefaultWorkers, "Workers")
	flag.Parse()

	ch := make(chan []Proxy, 30)
	ctx, cancel := context.WithTimeout(context.Background(), ScrapeTimeout)
	defer cancel()

	sources := 0
	if *protocol == "all" || *protocol == "http" {
		go scrapeProxyScrape(ctx, "http", *limit, ch); sources++
		go scrapeGitHubProxyLists(ctx, "thespeedx", *limit, ch); sources++
		go scrapeGitHubProxyLists(ctx, "monosans", *limit, ch); sources++
		go scrapeVakhov(ctx, *limit, ch); sources++
		go scrapeFreeProxyList(ctx, *limit, ch); sources++
		go scrapeGeoNodeAPI(ctx, "http", *limit, ch); sources++
	}
	if *protocol == "all" || *protocol == "socks5" {
		go scrapeProxyScrape(ctx, "socks5", *limit, ch); sources++
		go scrapeHookzof(ctx, *limit, ch); sources++
		go scrapeIplocate(ctx, *limit, ch); sources++
		go scrapeKomutan(ctx, *limit, ch); sources++
		go scrapeProxifly(ctx, *limit, ch); sources++
		go scrapeGeoNodeAPI(ctx, "socks5", *limit, ch); sources++
	}

	allProxies := []Proxy{}
collector:
	for i := 0; i < sources; i++ {
		select {
		case lst := <-ch:
			if lst != nil {
				allProxies = append(allProxies, lst...)
			}
		case <-ctx.Done():
			break collector
		}
	}

	seen := make(map[string]bool)
	unique := []Proxy{}
	for _, p := range allProxies {
		key := fmt.Sprintf("%s:%d", p.IP, p.Port)
		if !seen[key] {
			seen[key] = true
			unique = append(unique, p)
		}
	}

	validCh := make(chan Proxy, len(unique))
	var wg sync.WaitGroup
	sem := make(chan struct{}, *workers)
	for _, p := range unique {
		wg.Add(1)
		go func(proxy Proxy) {
			defer wg.Done()
			sem <- struct{}{}
			validateProxy(proxy, validCh)
			<-sem
		}(p)
	}
	go func() { wg.Wait(); close(validCh) }()

	validated := []Proxy{}
	for p := range validCh {
		if p.Latency > 0 && p.Type != "" {
			validated = append(validated, p)
		}
	}

	data, _ := json.MarshalIndent(validated, "", "  ")
	os.Stdout.Write(data)
}