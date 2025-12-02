-- 1. Enum para status e tipo de métrica
CREATE TYPE metric_status AS ENUM ('up', 'down', 'degraded', 'timeout');
CREATE TYPE metric_type AS ENUM ('ping', 'http', 'dns', 'tcp');

-- 2. Tabela de alvos monitorados (normalização)
CREATE TABLE monitoring_targets (
    id SERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    address TEXT NOT NULL,
    asn INTEGER,
    provider TEXT,
    type TEXT NOT NULL,
    region TEXT DEFAULT 'global',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(address)
);

-- 3. Tabela de probes (multi-localização)
CREATE TABLE monitoring_probes (
    id SERIAL PRIMARY KEY,
    location TEXT NOT NULL,
    ip_address INET,
    provider TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- 4. Tabela de ciclos de monitoramento (para consenso)
CREATE TABLE monitoring_cycles (
    id BIGSERIAL PRIMARY KEY,
    started_at TIMESTAMPTZ NOT NULL,
    ended_at TIMESTAMPTZ,
    cycle_number INTEGER NOT NULL,
    probe_count INTEGER NOT NULL DEFAULT 1
);

-- 5. Tabela principal de métricas (particionada por RANGE de data)
CREATE TABLE connectivity_metrics (
    id BIGSERIAL,
    cycle_id BIGINT REFERENCES monitoring_cycles(id),
    probe_id INTEGER REFERENCES monitoring_probes(id),
    target_id INTEGER REFERENCES monitoring_targets(id),
    timestamp TIMESTAMPTZ NOT NULL,
    metric_type metric_type NOT NULL,
    status metric_status NOT NULL,
    response_time_ms DOUBLE PRECISION,
    packet_loss_percent SMALLINT DEFAULT 0,
    error_message TEXT,
    PRIMARY KEY (id, timestamp)
);


-- 6. Tabela de eventos de outage (particionada por RANGE)
CREATE TABLE outage_events (
    id BIGSERIAL,
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ,
    duration_seconds INTEGER,
    reason TEXT,
    affected_targets INTEGER[] NOT NULL,
    affected_probes INTEGER[],
    consensus_level INTEGER DEFAULT 1,
    details JSONB,
    PRIMARY KEY (id, start_time)
);

-- 7. Índices otimizados para workloads de monitoramento
CREATE INDEX idx_metrics_time_target ON connectivity_metrics (timestamp DESC, target_id);
CREATE INDEX idx_metrics_status_time ON connectivity_metrics (status, timestamp DESC);
CREATE INDEX idx_metrics_type ON connectivity_metrics (metric_type);
CREATE INDEX idx_cycles_started_at ON monitoring_cycles (started_at);
CREATE INDEX idx_outage_time ON outage_events (start_time DESC);
CREATE INDEX idx_outage_duration ON outage_events (duration_seconds) WHERE duration_seconds IS NOT NULL;



-- Ingestão de dados

-- Cloudflare DNS
INSERT INTO monitoring_targets (name, address, asn, provider, type, region) VALUES
('Cloudflare DNS', '1.1.1.1', 13335, 'Cloudflare', 'dns', 'global'),
('Cloudflare DNS (Alt)', '1.0.0.1', 13335, 'Cloudflare', 'dns', 'global'),
('Cloudflare DNS (IPv6)', '2606:4700:4700::1111', 13335, 'Cloudflare', 'dns', 'global'),
('Cloudflare DNS (Alt IPv6)', '2606:4700:4700::1001', 13335, 'Cloudflare', 'dns', 'global'),

-- Google Public DNS
('Google Public DNS', '8.8.8.8', 15169, 'Google', 'dns', 'global'),
('Google Public DNS (Alt)', '8.8.4.4', 15169, 'Google', 'dns', 'global'),
('Google Public DNS (IPv6)', '2001:4860:4860::8888', 15169, 'Google', 'dns', 'global'),
('Google Public DNS (Alt IPv6)', '2001:4860:4860::8844', 15169, 'Google', 'dns', 'global'),

-- Quad9 DNS
('Quad9 DNS', '9.9.9.9', 19281, 'Quad9', 'dns', 'global'),
('Quad9 DNS (Alt)', '149.112.112.112', 19281, 'Quad9', 'dns', 'global'),
('Quad9 DNS (IPv6)', '2620:fe::fe', 19281, 'Quad9', 'dns', 'global'),
('Quad9 DNS (Alt IPv6)', '2620:fe::9', 19281, 'Quad9', 'dns', 'global'),

-- OpenDNS (Cisco Umbrella)
('OpenDNS', '208.67.222.222', 36692, 'Cisco', 'dns', 'global'),
('OpenDNS (Alt)', '208.67.220.220', 36692, 'Cisco', 'dns', 'global'),
('OpenDNS (IPv6)', '2620:119:35::35', 36692, 'Cisco', 'dns', 'global'),
('OpenDNS (Alt IPv6)', '2620:119:53::53', 36692, 'Cisco', 'dns', 'global'),

-- NextDNS
('NextDNS', '45.90.28.0', 208552, 'NextDNS', 'dns', 'global'),
('NextDNS (Alt)', '45.90.30.0', 208552, 'NextDNS', 'dns', 'global'),
('NextDNS (IPv6)', '2a07:a8c0::', 208552, 'NextDNS', 'dns', 'global'),
('NextDNS (Alt IPv6)', '2a07:a8c1::', 208552, 'NextDNS', 'dns', 'global'),

-- G-Core Labs DNS
('G-Core Labs DNS', '95.85.95.85', 199524, 'G-Core', 'dns', 'global'),
('G-Core Labs DNS (Alt)', '2.56.220.2', 199524, 'G-Core', 'dns', 'global'),
('G-Core Labs DNS (IPv6)', '2a03:90c0:999d::1', 199524, 'G-Core', 'dns', 'global'),
('G-Core Labs DNS (Alt IPv6)', '2a03:90c0:9992::1', 199524, 'G-Core', 'dns', 'global'),

-- Yandex.DNS
('Yandex.DNS', '77.88.8.8', 13238, 'Yandex', 'dns', 'eurasia'),
('Yandex.DNS (Alt)', '77.88.8.1', 13238, 'Yandex', 'dns', 'eurasia'),
('Yandex.DNS (IPv6)', '2a02:6b8::feed:0ff', 13238, 'Yandex', 'dns', 'eurasia'),
('Yandex.DNS (Alt IPv6)', '2a02:6b8:0:1::feed:0ff', 13238, 'Yandex', 'dns', 'eurasia'),

-- Registro.br (NIC.br) - Todos servidores autoritativos IPv4 e IPv6
('Registro.br a.dns.br', '200.160.0.10', 22548, 'NIC.br', 'dns', 'br'),
('Registro.br a.dns.br (IPv6)', '2001:12f8:6::10', 22548, 'NIC.br', 'dns', 'br'),
('Registro.br b.dns.br', '200.189.41.10', 22548, 'NIC.br', 'dns', 'br'),
('Registro.br b.dns.br (IPv6)', '2001:12f8:8::10', 22548, 'NIC.br', 'dns', 'br'),
('Registro.br c.dns.br', '200.192.233.10', 22548, 'NIC.br', 'dns', 'br'),
('Registro.br c.dns.br (IPv6)', '2001:12f8:a::10', 22548, 'NIC.br', 'dns', 'br'),
('Registro.br d.dns.br', '200.219.154.10', 22548, 'NIC.br', 'dns', 'br'),
('Registro.br d.dns.br (IPv6)', '2001:12f8:4::10', 22548, 'NIC.br', 'dns', 'br'),
('Registro.br e.dns.br', '200.229.248.10', 22548, 'NIC.br', 'dns', 'br'),
('Registro.br e.dns.br (IPv6)', '2001:12f8:2::10', 22548, 'NIC.br', 'dns', 'br'),

-- Akamai Edge (CDN) - Use hostname, não IP fixo
('Akamai Edge', 'a173-223-99-56.deploy.static.akamaitechnologies.com', NULL, 'Akamai', 'cdn', 'global'),

-- Amazon CloudFront (CDN) - Use hostname, não IP fixo
('Amazon CloudFront', 'd111111abcdef8.cloudfront.net', NULL, 'AWS', 'cdn', 'global');
