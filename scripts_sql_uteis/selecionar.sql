-- Queries SQL para Detectar e Analisar Micro-Quedas

-- a) Quedas já registradas (tabela outage_events)
SELECT 
    start_time,
    end_time,
    duration_seconds,
    ARRAY_LENGTH(affected_targets, 1) as num_targets_afetados,
    ARRAY_LENGTH(affected_probes, 1) as num_probes_afetados,
    consensus_level,
    reason,
    details
FROM outage_events 
WHERE duration_seconds BETWEEN 1 AND 3
ORDER BY start_time DESC
LIMIT 50;

-- a.2 ) Minha versão
SELECT 
    start_time,
    end_time,
    duration_seconds,
    ARRAY_LENGTH(affected_targets, 1) as num_targets_afetados,
    ARRAY_LENGTH(affected_probes, 1) as num_probes_afetados,
    consensus_level,
    reason,
    details
FROM outage_events 
WHERE duration_seconds <= 1
ORDER BY start_time DESC


-- b) Detecção granular de falhas rápidas (tabela connectivity_metrics)
WITH falhas_consecutivas AS (
    SELECT 
        cm.timestamp,
        cm.target_id,
        cm.probe_id,
        cm.status,
        cm.response_time_ms,
        mt.name as target_name,
        mp.location as probe_location,
        LAG(cm.status) OVER (
            PARTITION BY cm.target_id, cm.probe_id 
            ORDER BY cm.timestamp
        ) as status_anterior,
        LAG(cm.timestamp) OVER (
            PARTITION BY cm.target_id, cm.probe_id 
            ORDER BY cm.timestamp
        ) as timestamp_anterior
    FROM connectivity_metrics cm
    JOIN monitoring_targets mt ON cm.target_id = mt.id
    JOIN monitoring_probes mp ON cm.probe_id = mp.id
    WHERE cm.timestamp >= NOW() - INTERVAL '24 hours'
      AND cm.status IN ('down', 'timeout')
)
SELECT 
    target_name,
    probe_location,
    timestamp as momento_falha,
    EXTRACT(EPOCH FROM (timestamp - timestamp_anterior)) as intervalo_segundos,
    status,
    response_time_ms
FROM falhas_consecutivas 
WHERE status_anterior = 'up'
  AND EXTRACT(EPOCH FROM (timestamp - timestamp_anterior)) <= 60
ORDER BY timestamp DESC;

-- c) Análise de consenso entre probes
SELECT 
    DATE_TRUNC('minute', timestamp) as minuto,
    target_id,
    COUNT(*) as total_probes_testando,
    COUNT(CASE WHEN status IN ('down', 'timeout') THEN 1 END) as probes_falhando,
    ROUND(
        COUNT(CASE WHEN status IN ('down', 'timeout') THEN 1 END)::NUMERIC / 
        COUNT(*)::NUMERIC * 100, 1
    ) as percentual_falhas,
    ARRAY_AGG(DISTINCT probe_id) FILTER (WHERE status IN ('down', 'timeout')) as probes_com_falha
FROM connectivity_metrics 
WHERE timestamp >= NOW() - INTERVAL '1 hour'
GROUP BY DATE_TRUNC('minute', timestamp), target_id
HAVING COUNT(CASE WHEN status IN ('down', 'timeout') THEN 1 END) >= 2
ORDER BY minuto DESC, percentual_falhas DESC;
