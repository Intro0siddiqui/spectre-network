package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/gocolly/colly/v2"
)

type Proxy struct {
	IP       string  `json:"ip"`
	Port     int     `json:"port"`
	Type     string  `json:"type"`
	Latency  float64 `json:"latency,omitempty"`
	Country  string  `json:"country,omitempty"`
	Anonymity string `json:"anonymity,omitempty"`
}

func scrapeProxyScrape(protocol string, limit int, ch chan<- []Proxy) {
	defer close(ch)
	urlStr := fmt.Sprintf("https://api.proxyscrape.com/v2/?request=getproxies&protocol=%s&timeout=10000&country=all&ssl=all&anonymity=all", protocol)
	
	client := &http.Client{
		Timeout: 15 * time.Second,
	}
	resp, err := client.Get(urlStr)
	if err != nil {
		fmt.Printf("ProxyScrape (%s) failed: %v\n", protocol, err)
		return
	}
	defer resp.Body.Close()
	
	body, _ := io.ReadAll(resp.Body)
	lines := strings.Split(strings.TrimSpace(string(body)), "\n")
	proxies := make([]Proxy, 0, limit)
	
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if line == "" || !strings.Contains(line, ":") {
			continue
		}
		
		parts := strings.Split(line, ":")
		if len(parts) != 2 {
			continue
		}
		
		port, err := strconv.Atoi(parts[1])
		if err != nil {
			continue
		}
		
		proxies = append(proxies, Proxy{
			IP: parts[0], 
			Port: port, 
			Type: protocol,
			Anonymity: "anonymous",
		})
		
		if len(proxies) >= limit {
			break
		}
	}
	fmt.Printf("Scraped %d %s from ProxyScrape\n", len(proxies), protocol)
	ch <- proxies
}

func scrapeFreeProxyList(limit int, ch chan<- []Proxy) {
	defer close(ch)
	c := colly.NewCollector(
		colly.UserAgent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36"),
		colly.MaxDepth(1),
		colly.Timeout(30*time.Second),
	)
	
	proxies := make([]Proxy, 0, limit)
	
	c.OnHTML("table.table tbody tr", func(e *colly.HTMLElement) {
		if len(proxies) >= limit {
			return
		}
		
		ip := strings.TrimSpace(e.ChildText("td:nth-child(1)"))
		portStr := strings.TrimSpace(e.ChildText("td:nth-child(2)"))
		country := strings.TrimSpace(e.ChildText("td:nth-child(4)"))
		anonymity := strings.TrimSpace(e.ChildText("td:nth-child(5)"))
		isHTTPS := strings.TrimSpace(e.ChildText("td:nth-child(7)"))
		
		if ip == "" || portStr == "" {
			return
		}
		
		port, err := strconv.Atoi(portStr)
		if err != nil {
			return
		}
		
		ptype := "http"
		if isHTTPS == "yes" {
			ptype = "https"
		}
		
		proxies = append(proxies, Proxy{
			IP: ip,
			Port: port,
			Type: ptype,
			Country: country,
			Anonymity: anonymity,
		})
	})
	
	c.OnScraped(func(r *colly.Response) {
		fmt.Printf("Scraped %d from FreeProxyList\n", len(proxies))
		ch <- proxies
	})
	
	c.OnError(func(r *colly.Response, err error) {
		fmt.Printf("FreeProxyList error: %v\n", err)
		ch <- proxies // Return partial results
	})
	
	err := c.Visit("https://free-proxy-list.net/")
	if err != nil {
		fmt.Printf("FreeProxyList visit failed: %v\n", err)
	}
}

func scrapeSpysOne(limit int, ch chan<- []Proxy) {
	defer close(ch)
	c := colly.NewCollector(
		colly.UserAgent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36"),
		colly.MaxDepth(1),
		colly.Timeout(30*time.Second),
	)
	
	proxies := make([]Proxy, 0, limit)
	
	c.OnHTML("table tbody tr, table tr", func(e *colly.HTMLElement) {
		if len(proxies) >= limit {
			return
		}
		
		tds := e.ChildTexts("td")
		if len(tds) < 2 {
			return
		}
		
		ipPort := strings.TrimSpace(tds[0])
		if !strings.Contains(ipPort, ":") {
			return
		}
		
		parts := strings.Split(ipPort, ":")
		if len(parts) != 2 {
			return
		}
		
		port, err := strconv.Atoi(parts[1])
		if err != nil {
			return
		}
		
		ptype := "http"
		proxyTypeText := strings.ToLower(strings.TrimSpace(tds[1]))
		if strings.Contains(proxyTypeText, "socks") {
			if strings.Contains(proxyTypeText, "5") {
				ptype = "socks5"
			} else {
				ptype = "socks4"
			}
		}
		
		proxies = append(proxies, Proxy{
			IP: parts[0],
			Port: port,
			Type: ptype,
		})
	})
	
	c.OnScraped(func(r *colly.Response) {
		fmt.Printf("Scraped %d from Spys.one\n", len(proxies))
		ch <- proxies
	})
	
	c.OnError(func(r *colly.Response, err error) {
		fmt.Printf("Spys.one error: %v\n", err)
		ch <- proxies
	})
	
	err := c.Visit("http://spys.one/en/anonymous-proxy-list/")
	if err != nil {
		fmt.Printf("Spys.one visit failed: %v\n", err)
	}
}

func scrapeProxyNova(limit int, ch chan<- []Proxy) {
	defer close(ch)
	c := colly.NewCollector(
		colly.UserAgent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36"),
		colly.MaxDepth(1),
		colly.Timeout(30*time.Second),
	)
	
	proxies := make([]Proxy, 0, limit)
	
	c.OnHTML("table#tbl_proxy tbody tr", func(e *colly.HTMLElement) {
		if len(proxies) >= limit {
			return
		}
		
		ip := strings.TrimSpace(e.ChildText("td:nth-child(1)"))
		portStr := strings.TrimSpace(e.ChildText("td:nth-child(2)"))
		country := strings.TrimSpace(e.ChildText("td:nth-child(3)"))
		
		if ip == "" || portStr == "" {
			return
		}
		
		port, err := strconv.Atoi(portStr)
		if err != nil {
			return
		}
		
		proxies = append(proxies, Proxy{
			IP: ip,
			Port: port,
			Type: "http",
			Country: country,
		})
	})
	
	c.OnScraped(func(r *colly.Response) {
		fmt.Printf("Scraped %d from ProxyNova\n", len(proxies))
		ch <- proxies
	})
	
	c.OnError(func(r *colly.Response, err error) {
		fmt.Printf("ProxyNova error: %v\n", err)
		ch <- proxies
	})
	
	err := c.Visit("https://www.proxynova.com/proxy-server-list/")
	if err != nil {
		fmt.Printf("ProxyNova visit failed: %v\n", err)
	}
}

func scrapeProxifly(limit int, ch chan<- []Proxy) {
	defer close(ch)
	urls := []string{
		"https://raw.githubusercontent.com/proxifly/free-proxy-list/main/proxies/protocols/http/data.txt",
		"https://raw.githubusercontent.com/proxifly/free-proxy-list/main/proxies/protocols/https/data.txt",
		"https://raw.githubusercontent.com/proxifly/free-proxy-list/main/proxies/protocols/socks4/data.txt",
		"https://raw.githubusercontent.com/proxifly/free-proxy-list/main/proxies/protocols/socks5/data.txt",
	}
	
	client := &http.Client{Timeout: 15 * time.Second}
	proxies := make([]Proxy, 0, limit)
	
	for _, urlStr := range urls {
		if len(proxies) >= limit {
			break
		}
		
		var ptype string
		if strings.Contains(urlStr, "/http/") {
			ptype = "http"
		} else if strings.Contains(urlStr, "/https/") {
			ptype = "https"
		} else if strings.Contains(urlStr, "/socks4/") {
			ptype = "socks4"
		} else if strings.Contains(urlStr, "/socks5/") {
			ptype = "socks5"
		}
		
		resp, err := client.Get(urlStr)
		if err != nil {
			fmt.Printf("Proxifly %s failed: %v\n", ptype, err)
			continue
		}
		
		body, _ := io.ReadAll(resp.Body)
		resp.Body.Close()
		
		lines := strings.Split(strings.TrimSpace(string(body)), "\n")
		for _, line := range lines {
			if len(proxies) >= limit {
				break
			}
			
			line = strings.TrimSpace(line)
			if line == "" || !strings.Contains(line, ":") {
				continue
			}
			
			parts := strings.Split(line, ":")
			if len(parts) != 2 {
				continue
			}
			
			port, err := strconv.Atoi(parts[1])
			if err != nil {
				continue
			}
			
			proxies = append(proxies, Proxy{
				IP: parts[0],
				Port: port,
				Type: ptype,
			})
		}
	}
	
	fmt.Printf("Scraped %d from Proxifly\n", len(proxies))
	ch <- proxies
}

func scrapeOpenProxy(limit int, ch chan<- []Proxy) {
	defer close(ch)
	urls := map[string]string{
		"http":  "https://openproxy.space/list/http",
		"socks5": "https://openproxy.space/list/socks5",
	}
	
	client := &http.Client{Timeout: 15 * time.Second}
	proxies := make([]Proxy, 0, limit)
	
	for ptype, urlStr := range urls {
		if len(proxies) >= limit {
			break
		}
		
		resp, err := client.Get(urlStr)
		if err != nil {
			fmt.Printf("OpenProxy %s failed: %v\n", ptype, err)
			continue
		}
		
		body, _ := io.ReadAll(resp.Body)
		resp.Body.Close()
		
		lines := strings.Split(strings.TrimSpace(string(body)), "\n")
		for _, line := range lines {
			if len(proxies) >= limit {
				break
			}
			
			line = strings.TrimSpace(line)
			if line == "" || !strings.Contains(line, ":") {
				continue
			}
			
			parts := strings.Split(line, ":")
			if len(parts) != 2 {
				continue
			}
			
			port, err := strconv.Atoi(parts[1])
			if err != nil {
				continue
			}
			
			proxies = append(proxies, Proxy{
				IP: parts[0],
				Port: port,
				Type: ptype,
			})
		}
	}
	
	fmt.Printf("Scraped %d from OpenProxy\n", len(proxies))
	ch <- proxies
}

func scrapeProxyListDownload(protocol string, limit int, ch chan<- []Proxy) {
	defer close(ch)
	urlStr := fmt.Sprintf("https://www.proxy-list.download/api/v1/get?type=%s", protocol)
	
	client := &http.Client{Timeout: 15 * time.Second}
	resp, err := client.Get(urlStr)
	if err != nil {
		fmt.Printf("ProxyListDownload (%s) failed: %v\n", protocol, err)
		ch <- nil
		return
	}
	defer resp.Body.Close()
	
	body, _ := io.ReadAll(resp.Body)
	lines := strings.Split(strings.TrimSpace(string(body)), "\n")
	proxies := make([]Proxy, 0, limit)
	
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if line == "" || !strings.Contains(line, ":") {
			continue
		}
		
		parts := strings.Split(line, ":")
		if len(parts) != 2 {
			continue
		}
		
		port, err := strconv.Atoi(parts[1])
		if err != nil {
			continue
		}
		
		proxies = append(proxies, Proxy{
			IP: parts[0],
			Port: port,
			Type: protocol,
		})
		
		if len(proxies) >= limit {
			break
		}
	}
	
	fmt.Printf("Scraped %d %s from ProxyListDownload\n", len(proxies), protocol)
	ch <- proxies
}

func scrapeHideMyName(limit int, ch chan<- []Proxy) {
	defer close(ch)
	c := colly.NewCollector(
		colly.UserAgent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36"),
		colly.MaxDepth(1),
		colly.Timeout(30*time.Second),
	)
	
	proxies := make([]Proxy, 0, limit)
	
	c.OnHTML("table tbody tr", func(e *colly.HTMLElement) {
		if len(proxies) >= limit {
			return
		}
		
		tds := e.ChildTexts("td")
		if len(tds) < 2 {
			return
		}
		
		ipPort := strings.TrimSpace(tds[0])
		if !strings.Contains(ipPort, ":") {
			return
		}
		
		parts := strings.Split(ipPort, ":")
		if len(parts) != 2 {
			return
		}
		
		port, err := strconv.Atoi(parts[1])
		if err != nil {
			return
		}
		
		ptype := "http"
		if strings.Contains(strings.ToLower(tds[1]), "socks") {
			ptype = "socks5"
		}
		
		proxies = append(proxies, Proxy{
			IP: parts[0],
			Port: port,
			Type: ptype,
		})
	})
	
	c.OnScraped(func(r *colly.Response) {
		fmt.Printf("Scraped %d from HideMyName\n", len(proxies))
		ch <- proxies
	})
	
	c.OnError(func(r *colly.Response, err error) {
		fmt.Printf("HideMyName error: %v\n", err)
		ch <- proxies
	})
	
	err := c.Visit("https://hide.me/en/proxy-list/")
	if err != nil {
		fmt.Printf("HideMyName visit failed: %v\n", err)
	}
}

func scrapeFreeProxyWorld(protocol string, limit int, ch chan<- []Proxy) {
	defer close(ch)
	urlStr := fmt.Sprintf("https://freeproxy.world/?type=%s&page=1", protocol)
	
	client := &http.Client{Timeout: 15 * time.Second}
	resp, err := client.Get(urlStr)
	if err != nil {
		fmt.Printf("FreeProxyWorld (%s) failed: %v\n", protocol, err)
		ch <- nil
		return
	}
	defer resp.Body.Close()
	
	body, _ := io.ReadAll(resp.Body)
	lines := strings.Split(strings.TrimSpace(string(body)), "\n")
	proxies := make([]Proxy, 0, limit)
	
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if line == "" || !strings.Contains(line, ":") {
			continue
		}
		
		parts := strings.Split(line, ":")
		if len(parts) != 2 {
			continue
		}
		
		port, err := strconv.Atoi(parts[1])
		if err != nil {
			continue
		}
		
		proxies = append(proxies, Proxy{
			IP: parts[0],
			Port: port,
			Type: protocol,
		})
		
		if len(proxies) >= limit {
			break
		}
	}
	
	fmt.Printf("Scraped %d %s from FreeProxyWorld\n", len(proxies), protocol)
	ch <- proxies
}

// Real Proxy Platforms Integration

func scrapeWebshareAPI(limit int, ch chan<- []Proxy) {
	defer close(ch)
	// Webshare free API - limited but real proxies
	urlStr := "https://proxy.webshare.io/api/v2/proxy/list/?countries=US,CA,DE,NL,UK,FR&protocols=http,https,socks5&page=1&page_size=100"
	
	client := &http.Client{Timeout: 20 * time.Second}
	resp, err := client.Get(urlStr)
	if err != nil {
		fmt.Printf("Webshare API failed: %v\n", err)
		ch <- nil
		return
	}
	defer resp.Body.Close()
	
	body, _ := io.ReadAll(resp.Body)
	var data map[string]interface{}
	if err := json.Unmarshal(body, &data); err != nil {
		fmt.Printf("Webshare API parse failed: %v\n", err)
		ch <- nil
		return
	}
	
	proxies := make([]Proxy, 0, limit)
	proxiesList, ok := data["results"].([]interface{})
	if !ok {
		fmt.Printf("Webshare API invalid format\n")
		ch <- proxies
		return
	}
	
	for _, item := range proxiesList {
		if len(proxies) >= limit {
			break
		}
		
		proxyData, ok := item.(map[string]interface{})
		if !ok {
			continue
		}
		
		// Parse proxy data
		username := proxyData["username"].(string)
		password := proxyData["password"].(string)
		proxyAddr := proxyData["proxy_address"].(string)
		port := int(proxyData["port"].(float64))
		protocol := strings.ToLower(proxyData["protocol"].(string))
		
		if proxyAddr != "" && port > 0 {
			proxies = append(proxies, Proxy{
				IP: proxyAddr,
				Port: port,
				Type: protocol,
				Country: "US", // Webshare free tier is primarily US
			})
		}
	}
	
	fmt.Printf("Scraped %d from Webshare API\n", len(proxies))
	ch <- proxies
}

func scrapeGeoNodeAPI(protocol string, limit int, ch chan<- []Proxy) {
	defer close(ch)
	urlStr := fmt.Sprintf("https://proxylist.geonode.com/api/proxy-list?limit=%d&page=1&sort_by=lastChecked&sort_type=desc&protocols=%s", limit, protocol)
	
	client := &http.Client{Timeout: 15 * time.Second}
	resp, err := client.Get(urlStr)
	if err != nil {
		fmt.Printf("GeoNode API (%s) failed: %v\n", protocol, err)
		ch <- nil
		return
	}
	defer resp.Body.Close()
	
	body, _ := io.ReadAll(resp.Body)
	var data map[string]interface{}
	if err := json.Unmarshal(body, &data); err != nil {
		fmt.Printf("GeoNode API parse failed: %v\n", err)
		ch <- nil
		return
	}
	
	proxies := make([]Proxy, 0, limit)
	dataList, ok := data["data"].([]interface{})
	if !ok {
		fmt.Printf("GeoNode API invalid format\n")
		ch <- proxies
		return
	}
	
	for _, item := range dataList {
		if len(proxies) >= limit {
			break
		}
		
		proxyData, ok := item.(map[string]interface{})
		if !ok {
			continue
		}
		
		ip := proxyData["ip"].(string)
		port := int(proxyData["port"].(float64))
		ptype := strings.ToLower(proxyData["protocol"].(string))
		country := strings.ToUpper(proxyData["country"].(string))
		
		if ip != "" && port > 0 {
			proxies = append(proxies, Proxy{
				IP: ip,
				Port: port,
				Type: ptype,
				Country: country,
				Anonymity: "unknown",
			})
		}
	}
	
	fmt.Printf("Scraped %d %s from GeoNode API\n", len(proxies), protocol)
	ch <- proxies
}

func scrapeProxyScrapeTxtAPI(limit int, ch chan<- []Proxy) {
	defer close(ch)
	// Alternative proxy scraper API
	urlStr := fmt.Sprintf("https://api.proxyscrape.com/v2/", limit)
	
	client := &http.Client{Timeout: 15 * time.Second}
	
	// Get HTTP proxies
	resp, err := client.Get("https://api.proxyscrape.com/v2/?request=getproxies&protocol=http&timeout=10000&country=all&ssl=all&anonymity=all")
	if err == nil {
		defer resp.Body.Close()
		body, _ := io.ReadAll(resp.Body)
		lines := strings.Split(strings.TrimSpace(string(body)), "\n")
		
		for _, line := range lines {
			line = strings.TrimSpace(line)
			if line != "" && strings.Contains(line, ":") {
				parts := strings.Split(line, ":")
				if len(parts) == 2 {
					port, _ := strconv.Atoi(parts[1])
					ch <- []Proxy{{IP: parts[0], Port: port, Type: "http"}}
				}
			}
		}
	}
	
	// Get HTTPS proxies
	resp, err = client.Get("https://api.proxyscrape.com/v2/?request=getproxies&protocol=https&timeout=10000&country=all&ssl=all&anonymity=all")
	if err == nil {
		defer resp.Body.Close()
		body, _ := io.ReadAll(resp.Body)
		lines := strings.Split(strings.TrimSpace(string(body)), "\n")
		
		for _, line := range lines {
			line = strings.TrimSpace(line)
			if line != "" && strings.Contains(line, ":") {
				parts := strings.Split(line, ":")
				if len(parts) == 2 {
					port, _ := strconv.Atoi(parts[1])
					ch <- []Proxy{{IP: parts[0], Port: port, Type: "https"}}
				}
			}
		}
	}
	
	fmt.Printf("Scraped from ProxyScrapeTxt API\n")
}

func scrapeGitHubProxyLists(source string, limit int, ch chan<- []Proxy) {
	defer close(ch)
	
	var urls map[string]string
	switch source {
	case "thespeedx":
		urls = map[string]string{
			"http":  "https://raw.githubusercontent.com/TheSpeedX/PROXY-List/master/http.txt",
			"socks4": "https://raw.githubusercontent.com/TheSpeedX/PROXY-List/master/socks4.txt", 
			"socks5": "https://raw.githubusercontent.com/TheSpeedX/PROXY-List/master/socks5.txt",
		}
	case "monosans":
		urls = map[string]string{
			"http":  "https://raw.githubusercontent.com/monosans/proxy-list/main/proxies/http.txt",
			"socks5": "https://raw.githubusercontent.com/monosans/proxy-list/main/proxies/socks5.txt",
		}
	case "brianpoe":
		urls = map[string]string{
			"http":  "https://raw.githubusercontent.com/brianpoe/proxy-list/master/http.txt",
			"https": "https://raw.githubusercontent.com/brianpoe/proxy-list/master/https.txt",
			"socks4": "https://raw.githubusercontent.com/brianpoe/proxy-list/master/socks4.txt",
			"socks5": "https://raw.githubusercontent.com/brianpoe/proxy-list/master/socks5.txt",
		}
	default:
		ch <- nil
		return
	}
	
	client := &http.Client{Timeout: 20 * time.Second}
	allProxies := make([]Proxy, 0, limit)
	
	for protocol, urlStr := range urls {
		if len(allProxies) >= limit {
			break
		}
		
		resp, err := client.Get(urlStr)
		if err != nil {
			fmt.Printf("%s %s failed: %v\n", source, protocol, err)
			continue
		}
		
		body, _ := io.ReadAll(resp.Body)
		resp.Body.Close()
		
		lines := strings.Split(strings.TrimSpace(string(body)), "\n")
		for _, line := range lines {
			if len(allProxies) >= limit {
				break
			}
			
			line = strings.TrimSpace(line)
			if line == "" || !strings.Contains(line, ":") {
				continue
			}
			
			parts := strings.Split(line, ":")
			if len(parts) == 2 {
				port, err := strconv.Atoi(parts[1])
				if err != nil {
					continue
				}
				
				allProxies = append(allProxies, Proxy{
					IP: parts[0],
					Port: port,
					Type: protocol,
					Country: "GITHUB",
				})
			}
		}
	}
	
	fmt.Printf("Scraped %d from %s GitHub proxy lists\n", len(allProxies), source)
	ch <- allProxies
}

func scrapeProxyDaily(limit int, ch chan<- []Proxy) {
	defer close(ch)
	c := colly.NewCollector(
		colly.UserAgent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36"),
		colly.MaxDepth(1),
		colly.Timeout(30*time.Second),
	)
	
	proxies := make([]Proxy, 0, limit)
	
	c.OnHTML("table tbody tr", func(e *colly.HTMLElement) {
		if len(proxies) >= limit {
			return
		}
		
		ip := strings.TrimSpace(e.ChildText("td:nth-child(1)"))
		portStr := strings.TrimSpace(e.ChildText("td:nth-child(2)"))
		protocol := strings.TrimSpace(e.ChildText("td:nth-child(3)"))
		
		if ip != "" && portStr != "" {
			port, err := strconv.Atoi(portStr)
			if err != nil {
				return
			}
			
			proxies = append(proxies, Proxy{
				IP: ip,
				Port: port,
				Type: strings.ToLower(protocol),
				Country: "PROXYDAILY",
			})
		}
	})
	
	c.OnScraped(func(r *colly.Response) {
		fmt.Printf("Scraped %d from ProxyDaily\n", len(proxies))
		ch <- proxies
	})
	
	c.OnError(func(r *colly.Response, err error) {
		fmt.Printf("ProxyDaily error: %v\n", err)
		ch <- proxies
	})
	
	err := c.Visit("https://proxydaily.com/proxylist/")
	if err != nil {
		fmt.Printf("ProxyDaily visit failed: %v\n", err)
	}
}

func scrapeProxyMeshAPI(limit int, ch chan<- []Proxy) {
	defer close(ch)
	// ProxyMesh free API endpoint
	urlStr := fmt.Sprintf("https://proxymesh.com/api/proxy/?limit=%d", limit)
	
	client := &http.Client{Timeout: 15 * time.Second}
	resp, err := client.Get(urlStr)
	if err != nil {
		fmt.Printf("ProxyMesh API failed: %v\n", err)
		ch <- nil
		return
	}
	defer resp.Body.Close()
	
	body, _ := io.ReadAll(resp.Body)
	proxies := make([]Proxy, 0, limit)
	
	lines := strings.Split(strings.TrimSpace(string(body)), "\n")
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if line == "" || !strings.Contains(line, ":") {
			continue
		}
		
		parts := strings.Split(line, ":")
		if len(parts) >= 3 {
			// Format: ip:port:username:password
			ip := parts[0]
			port, _ := strconv.Atoi(parts[1])
			proxyType := "http"
			if len(parts) > 3 {
				proxyType = strings.ToLower(parts[3])
			}
			
			proxies = append(proxies, Proxy{
				IP: ip,
				Port: port,
				Type: proxyType,
				Country: "PROXYMESH",
			})
		}
	}
	
	fmt.Printf("Scraped %d from ProxyMesh API\n", len(proxies))
	ch <- proxies
}

func validateProxy(p Proxy, ch chan<- Proxy, wg *sync.WaitGroup) {
	defer wg.Done()
	
	// Create proxy URL
	proxyURL := url.URL{Scheme: p.Type, Host: fmt.Sprintf("%s:%d", p.IP, p.Port)}
	
	// Create HTTP client with proxy
	client := &http.Client{
		Timeout: 8 * time.Second,
		Transport: &http.Transport{
			Proxy: http.ProxyURL(&proxyURL),
		},
		CheckRedirect: func(req *http.Request, via []*http.Request) error {
			return http.ErrUseLastResponse
		},
	}
	
	// Test proxy by checking IP
	start := time.Now()
	resp, err := client.Get("https://httpbin.org/ip")
	latency := time.Since(start).Seconds()
	
	if err != nil || resp.StatusCode != 200 {
		// Proxy failed, return with 0 latency
		ch <- Proxy{IP: p.IP, Port: p.Port, Type: p.Type, Latency: 0}
		if resp != nil {
			resp.Body.Close()
		}
		return
	}
	
	defer resp.Body.Close()
	
	// Parse response to get returned IP
	var result map[string]interface{}
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		ch <- Proxy{IP: p.IP, Port: p.Port, Type: p.Type, Latency: 0}
		return
	}
	
	// If we got here, proxy worked
	ch <- Proxy{
		IP: p.IP,
		Port: p.Port,
		Type: p.Type,
		Latency: latency,
		Country: p.Country,
		Anonymity: p.Anonymity,
	}
}

func main() {
	protocol := flag.String("protocol", "all", "Proxy protocol: all, http, https, socks4, socks5")
	limit := flag.Int("limit", 500, "Max proxies to scrape and validate")
	workers := flag.Int("workers", 100, "Number of concurrent validation workers")
	flag.Parse()

	fmt.Printf("Spectre Network Proxy Scraper v1.0\n")
	fmt.Printf("Protocol: %s, Limit: %d, Workers: %d\n\n", *protocol, *limit, *workers)

	// Create channels for scraping results
	ch := make(chan []Proxy, 15) // Increased for more real sources
	
	// Kick off scraping based on protocol
	if *protocol == "all" {
		// Run all scrapers with balanced distribution - REAL PROXY SOURCES
		perSource := *limit / 18
		go scrapeProxyScrape("http", perSource, ch)
		go scrapeProxyScrape("socks5", perSource, ch)
		go scrapeGeoNodeAPI("http", perSource, ch)
		go scrapeGeoNodeAPI("socks5", perSource, ch)
		go scrapeGitHubProxyLists("thespeedx", perSource, ch)
		go scrapeGitHubProxyLists("monosans", perSource, ch)
		go scrapeMoreGitHubProxies(perSource, ch)
		go scrapeProxyScrapeLive(perSource, ch)
		go scrapeProxyDaily(perSource, ch)
		go scrapeFreeProxyList(perSource, ch)
		go scrapeSpysOne(perSource, ch)
		go scrapeProxyNova(perSource, ch)
		go scrapeProxifly(perSource, ch)
		go scrapeOpenProxy(perSource, ch)
		go scrapeProxyListDownload("http", perSource, ch)
		go scrapeHideMyName(perSource, ch)
		go scrapeFreeProxyWorld("http", perSource, ch)
		
		// Add premium real sources (commented out as they need API keys)
		// go scrapeWebshareAPI(perSource, ch)
		// go scrapeProxyMeshAPI(perSource, ch)
		
	} else {
		// Run specific protocol scrapers with REAL SOURCES
		perSource := *limit / 12
		switch *protocol {
		case "http", "https":
			go scrapeProxyScrape(*protocol, perSource, ch)
			go scrapeGeoNodeAPI(*protocol, perSource, ch)
			go scrapeGitHubProxyLists("thespeedx", perSource, ch)
			go scrapeGitHubProxyLists("monosans", perSource, ch)
			go scrapeMoreGitHubProxies(perSource, ch)
			go scrapeProxyListDownload(*protocol, perSource, ch)
			go scrapeFreeProxyWorld(*protocol, perSource, ch)
			go scrapeOpenProxy(*protocol, ch)
			go scrapeProxifly(perSource, ch)
			go scrapeFreeProxyList(perSource, ch)
			go scrapeProxyNova(perSource, ch)
			go scrapeProxyDaily(perSource, ch)
			// go scrapeWebshareAPI(perSource, ch)
		case "socks4", "socks5":
			go scrapeProxyScrape(*protocol, perSource, ch)
			go scrapeGeoNodeAPI(*protocol, perSource, ch)
			go scrapeGitHubProxyLists("thespeedx", perSource, ch)
			go scrapeGitHubProxyLists("monosans", perSource, ch)
			go scrapeMoreGitHubProxies(perSource, ch)
			go scrapeOpenProxy(*protocol, ch)
			go scrapeProxifly(perSource, ch)
			go scrapeSpysOne(perSource, ch)
			go scrapeFreeProxyList(perSource, ch)
			go scrapeProxyNova(perSource, ch)
			go scrapeHideMyName(perSource, ch)
			go scrapeProxyDaily(perSource, ch)
			// go scrapeProxyMeshAPI(perSource, ch)
		}
	}

	// Collect results from all scrapers
	allProxies := []Proxy{}
	fmt.Println("Collecting proxy lists from REAL proxy sources...")
	
	// Determine expected number of sources
	expectedSources := 18
	if *protocol != "all" {
		expectedSources = 12
	}
	
	for i := 0; i < expectedSources; i++ {
		lst := <-ch
		if lst != nil {
			allProxies = append(allProxies, lst...)
		}
	}

	fmt.Printf("Total raw proxies collected: %d\n", len(allProxies))
	
	// Deduplicate proxies by IP:Port
	seen := make(map[string]bool)
	uniqueProxies := []Proxy{}
	for _, p := range allProxies {
		key := fmt.Sprintf("%s:%d", p.IP, p.Port)
		if !seen[key] {
			seen[key] = true
			uniqueProxies = append(uniqueProxies, p)
		}
	}
	
	fmt.Printf("Unique proxies after deduplication: %d\n", len(uniqueProxies))
	
	// Validate proxies concurrently
	fmt.Printf("Starting validation with %d workers...\n", *workers)
	validCh := make(chan Proxy, len(uniqueProxies))
	var wg sync.WaitGroup
	
	// Limit concurrent validations
	sem := make(chan struct{}, *workers)
	
	for _, p := range uniqueProxies {
		wg.Add(1)
		go func(proxy Proxy) {
			defer wg.Done()
			sem <- struct{}{} // Acquire semaphore
			defer func() { <-sem }() // Release semaphore
			
			validateProxy(proxy, validCh, &wg)
		}(p)
	}
	
	// Wait for all validations to complete
	go func() {
		wg.Wait()
		close(validCh)
	}()
	
	// Collect validated proxies
	validatedProxies := []Proxy{}
	for p := range validCh {
		if p.Latency > 0 { // Only include working proxies
			validatedProxies = append(validatedProxies, p)
		}
	}
	
	// Sort by latency (fastest first)
	if len(validatedProxies) > 1 {
		quickSort(validatedProxies, 0, len(validatedProxies)-1)
	}
	
	// Limit to requested amount
	if len(validatedProxies) > *limit {
		validatedProxies = validatedProxies[:*limit]
	}
	
	// Output results
	data, err := json.MarshalIndent(validatedProxies, "", "  ")
	if err != nil {
		fmt.Printf("Error encoding JSON: %v\n", err)
		os.Exit(1)
	}
	
	fmt.Printf("\nValidated proxies: %d\n", len(validatedProxies))
	fmt.Printf("Success rate: %.2f%%\n", float64(len(validatedProxies))/float64(len(uniqueProxies))*100)
	
	// Write to file if not just showing stdout
	os.Stdout.Write(data)
	fmt.Println()
}

func quickSort(proxies []Proxy, low, high int) {
	if low < high {
		pi := partition(proxies, low, high)
		quickSort(proxies, low, pi-1)
		quickSort(proxies, pi+1, high)
	}
}

func partition(proxies []Proxy, low, high int) int {
	pivot := proxies[high].Latency
	i := low
	for j := low; j < high; j++ {
		if proxies[j].Latency < pivot {
			proxies[i], proxies[j] = proxies[j], proxies[i]
			i++
		}
	}
	proxies[i], proxies[high] = proxies[high], proxies[i]
	return i
}

// Additional Real Proxy Sources

func scrapeMoreGitHubProxies(limit int, ch chan<- []Proxy) {
	defer close(ch)
	
	// ProxyFish GitHub repository
	urls := map[string]string{
		"http":  "https://raw.githubusercontent.com/ProxyFish/proxy-list/main/http.txt",
		"https": "https://raw.githubusercontent.com/ProxyFish/proxy-list/main/https.txt", 
		"socks4": "https://raw.githubusercontent.com/ProxyFish/proxy-list/main/socks4.txt",
		"socks5": "https://raw.githubusercontent.com/ProxyFish/proxy-list/main/socks5.txt",
	}
	
	client := &http.Client{Timeout: 20 * time.Second)
	allProxies := make([]Proxy, 0, limit)
	
	for protocol, urlStr := range urls {
		if len(allProxies) >= limit {
			break
		}
		
		resp, err := client.Get(urlStr)
		if err != nil {
			fmt.Printf("ProxyFish %s failed: %v\n", protocol, err)
			continue
		}
		
		body, _ := io.ReadAll(resp.Body)
		resp.Body.Close()
		
		lines := strings.Split(strings.TrimSpace(string(body)), "\n")
		for _, line := range lines {
			if len(allProxies) >= limit {
				break
			}
			
			line = strings.TrimSpace(line)
			if line == "" || !strings.Contains(line, ":") {
				continue
			}
			
			parts := strings.Split(line, ":")
			if len(parts) == 2 {
				port, _ := strconv.Atoi(parts[1])
				allProxies = append(allProxies, Proxy{
					IP: parts[0],
					Port: port,
					Type: protocol,
					Country: "PROXYFISH",
				})
			}
		}
	}
	
	fmt.Printf("Scraped %d from ProxyFish GitHub\n", len(allProxies))
	ch <- allProxies
}

func scrapeProxyScrapeLive(limit int, ch chan<- []Proxy) {
	defer close(ch)
	
	// More comprehensive ProxyScrape API with different parameters
	apis := []string{
		"https://api.proxyscrape.com/v2/?request=getproxies&protocol=http&timeout=10000&country=all&ssl=all&anonymity=elite",
		"https://api.proxyscrape.com/v2/?request=getproxies&protocol=https&timeout=10000&country=all&ssl=all&anonymity=anonymous",
		"https://api.proxyscrape.com/v2/?request=getproxies&protocol=socks5&timeout=10000&country=all&ssl=all&anonymity=all",
	}
	
	client := &http.Client{Timeout: 15 * time.Second)
	allProxies := make([]Proxy, 0, limit)
	
	for i, urlStr := range apis {
		if len(allProxies) >= limit {
			break
		}
		
		resp, err := client.Get(urlStr)
		if err != nil {
			fmt.Printf("ProxyScrape API %d failed: %v\n", i+1, err)
			continue
		}
		
		body, _ := io.ReadAll(resp.Body)
		resp.Body.Close()
		
		lines := strings.Split(strings.TrimSpace(string(body)), "\n")
		for _, line := range lines {
			if len(allProxies) >= limit {
				break
			}
			
			line = strings.TrimSpace(line)
			if line == "" || !strings.Contains(line, ":") {
				continue
			}
			
			parts := strings.Split(line, ":")
			if len(parts) == 2 {
				port, _ := strconv.Atoi(parts[1])
				var ptype string
				if i == 0 {
					ptype = "http"
				} else if i == 1 {
					ptype = "https"
				} else {
					ptype = "socks5"
				}
				
				allProxies = append(allProxies, Proxy{
					IP: parts[0],
					Port: port,
					Type: ptype,
					Country: "PROXYSCRAPE",
				})
			}
		}
	}
	
	fmt.Printf("Scraped %d from ProxyScrape Live APIs\n", len(allProxies))
	ch <- allProxies
}