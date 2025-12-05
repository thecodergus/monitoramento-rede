-- 1. Enum para status e tipo de métrica (granular IPv4/IPv6)
CREATE TYPE metric_status AS ENUM ('up', 'down', 'degraded', 'timeout');
CREATE TYPE metric_type AS ENUM (
    'ping_ipv4', 'ping_ipv6',
    'tcp_ipv4', 'tcp_ipv6',
    'http_ipv4', 'http_ipv6',
    'dns_ipv4', 'dns_ipv6'
);

-- 2. Tabela de alvos monitorados (normalização)
CREATE TABLE monitoring_targets (
    id SERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    address INET NOT NULL, -- Suporte nativo a IPv4/IPv6
    asn INTEGER,
    provider TEXT,
    type TEXT NOT NULL, -- Ex: 'dns_ipv4', 'dns_ipv6', 'tcp_ipv4', etc.
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

-- 7. Tabela de status do alvo
CREATE TABLE target_status (
    target_id INTEGER PRIMARY KEY,
    last_status metric_status NOT NULL,
    last_change TIMESTAMP NOT NULL
);

-- 8. Índices otimizados para workloads de monitoramento
CREATE INDEX idx_metrics_time_target ON connectivity_metrics (timestamp DESC, target_id);
CREATE INDEX idx_metrics_status_time ON connectivity_metrics (status, timestamp DESC);
CREATE INDEX idx_metrics_type ON connectivity_metrics (metric_type);
CREATE INDEX idx_cycles_started_at ON monitoring_cycles (started_at);
CREATE INDEX idx_outage_time ON outage_events (start_time DESC);
CREATE INDEX idx_outage_duration ON outage_events (duration_seconds) WHERE duration_seconds IS NOT NULL;
CREATE INDEX idx_metrics_brin_time ON connectivity_metrics USING BRIN (timestamp);

-- 9. Ingestão de dados de exemplo

-- DNS públicos e Registro.br (IPv4 e IPv6)
INSERT INTO monitoring_targets (name, address, asn, provider, type, region) VALUES
('Google Public DNS', '8.8.8.8', 15169, 'Google', 'dns_ipv4', 'global'),
('OpenDNS', '208.67.222.222', 36692, 'Cisco', 'dns_ipv4', 'global'),
('Google Public DNS (IPv6)', '2001:4860:4860::8888', 15169, 'Google', 'dns_ipv6', 'global'),
('OpenDNS (IPv6)', '2620:119:35::35', 36692, 'Cisco', 'dns_ipv6', 'global'),
('Google Public DNS (Alt IPv6)', '2001:4860:4860::8844', 15169, 'Google', 'dns_ipv6', 'global'),
('OpenDNS (Alt)', '208.67.220.220', 36692, 'Cisco', 'dns_ipv4', 'global'),
('Google Public DNS (Alt)', '8.8.4.4', 15169, 'Google', 'dns_ipv4', 'global'),
('OpenDNS (Alt IPv6)', '2620:119:53::53', 36692, 'Cisco', 'dns_ipv6', 'global'),

('Registro.br a.dns.br', '200.160.0.10', 22548, 'NIC.br', 'dns_ipv4', 'br'),
('Registro.br a.dns.br (IPv6)', '2001:12f8:6::10', 22548, 'NIC.br', 'dns_ipv6', 'br'),
('Registro.br b.dns.br', '200.189.41.10', 22548, 'NIC.br', 'dns_ipv4', 'br'),
('Registro.br b.dns.br (IPv6)', '2001:12f8:8::10', 22548, 'NIC.br', 'dns_ipv6', 'br'),
('Registro.br c.dns.br', '200.192.233.10', 22548, 'NIC.br', 'dns_ipv4', 'br'),
('Registro.br c.dns.br (IPv6)', '2001:12f8:a::10', 22548, 'NIC.br', 'dns_ipv6', 'br'),
('Registro.br d.dns.br', '200.219.154.10', 22548, 'NIC.br', 'dns_ipv4', 'br'),
('Registro.br d.dns.br (IPv6)', '2001:12f8:4::10', 22548, 'NIC.br', 'dns_ipv6', 'br'),
('Registro.br e.dns.br', '200.229.248.10', 22548, 'NIC.br', 'dns_ipv4', 'br'),
('Registro.br e.dns.br (IPv6)', '2001:12f8:2::10', 22548, 'NIC.br', 'dns_ipv6', 'br');

-- SOUTH AMERICA (5 probes)
INSERT INTO monitoring_probes (location, ip_address, provider) VALUES
('São Paulo - AWS sa-east-1', '18.228.0.1', 'AWS'),
('São Paulo - AWS sa-east-1 IPv6', '2600:1f18:6ff:f000::1', 'AWS'),
('São Paulo - GCP southamerica-east1', '35.198.10.1', 'GCP'),
('Rio de Janeiro - Datacenter Próprio', '200.160.10.1', 'Internal'),
('Buenos Aires - DigitalOcean', '157.230.200.1', 'DigitalOcean');

-- NORTH AMERICA (7 probes)
INSERT INTO monitoring_probes (location, ip_address, provider) VALUES
('Virginia - AWS us-east-1', '3.80.0.1', 'AWS'),
('Virginia - AWS us-east-1 IPv6', '2600:1f18:400:8000::1', 'AWS'),
('Oregon - GCP us-west1', '34.102.0.1', 'GCP'),
('Texas - Azure South Central US', '20.185.0.1', 'Azure'),
('New York - DigitalOcean', '157.230.123.1', 'DigitalOcean'),
('Toronto - Linode', '172.105.25.1', 'Linode'),
('Miami - CDN Edge', '23.236.0.1', 'CDN');

-- WESTERN EUROPE (6 probes)
INSERT INTO monitoring_probes (location, ip_address, provider) VALUES
('Frankfurt - AWS eu-central-1 IPv6', '2a05:d018:400:8000::1', 'AWS'),
('London - Azure UK South', '51.140.0.1', 'Azure'),
('Amsterdam - GCP europe-west4', '35.204.0.1', 'GCP'),
('Paris - Linode', '172.105.67.1', 'Linode'),
('Dublin - CDN Edge', '54.246.0.1', 'CDN');
