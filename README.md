# Network Monitoring System

## Autor
**Gustavo Michels de Camargo**

---

## Sobre o Projeto

Este projeto é um sistema de monitoramento de rede de alta performance, desenvolvido em Rust, que realiza verificações contínuas de disponibilidade e latência em múltiplos alvos de rede (targets) utilizando múltiplos probes distribuídos. O sistema detecta falhas e degradações de serviço por meio de algoritmos de consenso, armazena métricas detalhadas em banco de dados PostgreSQL e oferece arquitetura robusta, escalável e auditável para ambientes críticos.

---

## Como Funciona

- **Monitoramento em Tempo Real:** Probes executam pings e outros testes de conectividade em targets configurados, a cada ciclo definido (ex: 1 segundo).
- **Detecção de Outages por Consenso:** O sistema só considera um alvo como "fora do ar" se múltiplos probes concordarem, reduzindo falsos positivos.
- **Persistência de Métricas:** Todos os resultados de testes são armazenados na tabela `connectivity_metrics` do PostgreSQL, permitindo análises históricas, relatórios de SLA e troubleshooting.
- **Gerenciamento de Outages:** Eventos de indisponibilidade são registrados com início, fim e duração, facilitando auditoria e relatórios.
- **Configuração Flexível:** Parâmetros como frequência de monitoramento, thresholds de falha, consenso e targets são facilmente ajustáveis via arquivo `config.toml`.
- **Escalabilidade e Performance:** Uso intensivo de programação assíncrona (Tokio), batch inserts, pooling de conexões e particionamento de dados para suportar grandes volumes sem degradação.

---

## Arquitetura e Implementação

### Stack Tecnológica

- **Linguagem:** Rust (Edition 2021+)
- **Runtime Assíncrono:** Tokio
- **Banco de Dados:** PostgreSQL
- **Containerização:** Docker & Docker Compose
- **Configuração:** TOML (`config.toml`)

### Estrutura de Diretórios

```
codagem/
├── src/
│   ├── main.rs          # Ponto de entrada da aplicação
│   ├── config.rs        # Carregamento e validação de configuração
│   ├── consensus.rs     # Algoritmo de consenso para outages
│   ├── outage.rs        # Gerenciamento de eventos de outage
│   ├── ping.rs          # Operações de ping e coleta de métricas
│   ├── scheduler.rs     # Agendamento dos ciclos de monitoramento
│   ├── storage.rs       # Integração com PostgreSQL
│   ├── types.rs         # Estruturas de dados e tipos
│   └── warmup.rs        # Lógica de warmup dos targets
├── Cargo.toml           # Configuração do pacote Rust
├── config.toml          # Configuração da aplicação
docker/
├── monitor.dockerfile   # Dockerfile da aplicação
└── postgres/
    └── init.sql         # Script de inicialização do banco
scripts_sql_uteis/
├── delete_all.sql       # Limpeza de dados
└── selecionar.sql       # Consultas utilitárias
docker-compose.yml       # Orquestração dos serviços
```

### Principais Módulos

- **config.rs:** Carrega e valida parâmetros do sistema.
- **consensus.rs:** Implementa lógica de consenso para detecção de falhas reais.
- **ping.rs:** Realiza testes de conectividade (ping, etc.) de forma concorrente.
- **scheduler.rs:** Orquestra os ciclos de monitoramento e coordena os módulos.
- **storage.rs:** Gerencia persistência de métricas, outages e estados no PostgreSQL.
- **outage.rs:** Detecta, inicia e encerra eventos de outage.
- **types.rs:** Define as estruturas de dados centrais do sistema.

---

## Banco de Dados

- **Tabela principal:** `connectivity_metrics`
  - Armazena todos os resultados de testes de conectividade (ping, http, etc.)
  - Campos: ciclo, probe, target, timestamp, tipo de métrica, status, latência, perda de pacotes, mensagem de erro.
- **Outras tabelas:** `outages`, `probes`, `targets`, `cycles`
- **Scripts de inicialização:** `docker/postgres/init.sql`
- **Scripts utilitários:** `scripts_sql_uteis/`

---

## Configuração

Edite o arquivo `codagem/config.toml` para ajustar os parâmetros do sistema:

```toml
# Exemplo de configuração
ping_count = 3
ping_timeout = 5000
fail_threshold = 2
consensus_level = 0.6
cycle_interval = 1
database_url = "postgresql://usuario:senha@localhost/monitoring"
```

- `ping_count`: Número de tentativas de ping por ciclo
- `ping_timeout`: Timeout de cada ping (ms)
- `fail_threshold`: Falhas antes de considerar DOWN
- `consensus_level`: Percentual mínimo de probes para consenso de outage
- `cycle_interval`: Intervalo entre ciclos (segundos)
- `database_url`: String de conexão PostgreSQL

---

## Como Utilizar

### 1. Pré-requisitos

- Rust (versão estável mais recente)
- Docker e Docker Compose
- PostgreSQL (caso não use Docker)

### 2. Instalação e Inicialização

**Com Docker (recomendado):**

```bash
git clone <url-do-repositorio>
cd <diretorio-do-projeto>
cp codagem/config.toml.example codagem/config.toml
# Edite o config.toml conforme necessário
docker-compose up -d
```

**Manual:**

```bash
# Instale o Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Instale e configure o PostgreSQL
createdb monitoring
psql monitoring < docker/postgres/init.sql

# Compile e execute a aplicação
cd codagem
cargo build --release
cargo run
```

### 3. Operação

- O sistema inicia automaticamente o monitoramento dos targets configurados.
- Métricas e eventos são persistidos no banco e podem ser consultados via SQL ou dashboards externos.
- Logs detalhados são emitidos para acompanhamento e troubleshooting.

### 4. Consultas e Manutenção

- Use os scripts em `scripts_sql_uteis/` para consultas rápidas ou limpeza de dados.
- Para remover dados antigos e liberar espaço, utilize particionamento e políticas de retenção no PostgreSQL.

---

## Otimização e Escalabilidade

- **Particionamento:** Recomenda-se particionar a tabela `connectivity_metrics` por mês para facilitar limpeza e retenção.
- **Batch Inserts:** O sistema pode ser ajustado para inserir métricas em lotes, reduzindo overhead.
- **Pooling de Conexões:** Uso de pools de conexão para alta concorrência.
- **Retenção:** Automatize a remoção de partições antigas para evitar crescimento indefinido do banco.

---

## Desenvolvimento

- **Build:** `cargo build`
- **Testes:** `cargo test`
- **Lint:** `cargo clippy`
- **Format:** `cargo fmt`

---

## Licença e Suporte

Projeto desenvolvido e mantido por **Gustavo Michels de Camargo**.

Para dúvidas, sugestões ou contribuições, consulte a documentação do projeto ou entre em contato com o autor.

---

**Nota:**  
Este sistema foi projetado para ambientes de missão crítica, com foco em perstgreSQL.
