#!/usr/bin/env bash
# Failure-injection test (DESIGN.md §6): kill Logstash mid-stream, publish
# events, and assert zero loss once it recovers. Run against a healthy stack:
#   docker compose up -d && ./scripts/no-data-loss-test.sh
set -euo pipefail

API="${API:-http://localhost:8080}"

es_count() {
  docker compose exec -T elasticsearch \
    curl -s 'localhost:9200/weather-recs-2*/_count?ignore_unavailable=true' \
    | python3 -c 'import json,sys; print(json.load(sys.stdin).get("count", 0))'
}

queue_depth() {
  docker compose exec -T rabbitmq rabbitmqctl list_queues messages --quiet 2>/dev/null | tail -1
}

echo "==> Baseline"
before=$(es_count)
echo "    Elasticsearch documents: $before"

echo "==> Stopping Logstash (simulated processor outage)"
docker compose stop logstash >/dev/null

echo "==> Publishing 3 evaluations while the processor is down"
for activity in matkot nature gaming; do
  published=$(curl -sf -X POST "$API/api/evaluate" \
    -H 'Content-Type: application/json' \
    -d "{\"city\":\"Haifa\",\"activity\":\"$activity\"}" \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["published"])')
  echo "    $activity published=$published"
  [ "$published" = "True" ] || { echo "FAIL: event was not durably published"; exit 1; }
done

# >= 3: the scheduler may legitimately publish its own events concurrently.
depth=$(queue_depth)
echo "    RabbitMQ backlog: $depth messages"
[ "$depth" -ge 3 ] || { echo "FAIL: expected >= 3 queued messages, saw $depth"; exit 1; }

echo "==> Restarting Logstash"
docker compose start logstash >/dev/null

expected=$((before + 3))
echo "==> Waiting for the backlog to drain (expect $expected documents)"
for _ in $(seq 1 60); do
  after=$(es_count)
  [ "$after" -ge "$expected" ] && break
  sleep 5
done

after=$(es_count)
depth=$(queue_depth)
echo "    Elasticsearch documents: $after, queue depth: $depth"
if [ "$after" -ge "$expected" ] && [ "$depth" -eq 0 ]; then
  echo "PASS: zero data loss across the outage"
else
  echo "FAIL: expected >= $expected docs and empty queue"
  exit 1
fi
