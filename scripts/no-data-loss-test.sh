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

# Exact per-document check: events index with document_id == event_id, so the
# presence of a specific id proves the specific injected event survived —
# aggregate counts could pass even if one event were lost while another landed.
es_has_event() {
  docker compose exec -T elasticsearch \
    curl -s "localhost:9200/weather-recs-2*/_count?ignore_unavailable=true&q=event_id:%22$1%22" \
    | python3 -c 'import json,sys; print(json.load(sys.stdin).get("count", 0))'
}

queue_depth() {
  docker compose exec -T rabbitmq rabbitmqctl list_queues messages --quiet 2>/dev/null | tail -1
}

# Stop the scheduler for the test's duration so no concurrent publishes blur
# the arithmetic: every assertion below can then be exact instead of >=-loose.
echo "==> Pausing the scheduler for an interference-free window"
docker compose stop scheduler >/dev/null
trap 'docker compose start scheduler >/dev/null' EXIT

echo "==> Baseline"
before=$(es_count)
echo "    Elasticsearch documents: $before"

echo "==> Stopping Logstash (simulated processor outage)"
docker compose stop logstash >/dev/null

echo "==> Publishing 3 evaluations while the processor is down"
event_ids=()
for activity in matkot nature gaming; do
  response=$(curl -sf -X POST "$API/api/evaluate" \
    -H 'Content-Type: application/json' \
    -d "{\"city\":\"Haifa\",\"activity\":\"$activity\"}")
  published=$(echo "$response" | python3 -c 'import json,sys; print(json.load(sys.stdin)["published"])')
  event_id=$(echo "$response" | python3 -c 'import json,sys; print(json.load(sys.stdin)["event"]["event_id"])')
  echo "    $activity published=$published event_id=$event_id"
  [ "$published" = "True" ] || { echo "FAIL: event was not durably published"; exit 1; }
  event_ids+=("$event_id")
done

depth=$(queue_depth)
echo "    RabbitMQ backlog: $depth messages"
[ "$depth" -eq 3 ] || { echo "FAIL: expected exactly 3 queued messages, saw $depth"; exit 1; }

echo "==> Restarting Logstash"
docker compose start logstash >/dev/null

echo "==> Waiting for the backlog to drain"
for _ in $(seq 1 60); do
  [ "$(queue_depth)" -eq 0 ] && break
  sleep 5
done

echo "==> Waiting for the injected events to be indexed"
all=0
for _ in $(seq 1 12); do
  all=1
  for id in "${event_ids[@]}"; do
    [ "$(es_has_event "$id")" -eq 1 ] || { all=0; break; }
  done
  [ "$all" -eq 1 ] && break
  sleep 5
done

for id in "${event_ids[@]}"; do
  echo "    event $id indexed: $(es_has_event "$id")"
done
depth=$(queue_depth)
echo "    Elasticsearch documents: $(es_count), queue depth: $depth"
if [ "$all" -eq 1 ] && [ "$depth" -eq 0 ]; then
  echo "PASS: zero data loss across the outage"
else
  echo "FAIL: an injected event is missing from Elasticsearch or the queue did not drain"
  exit 1
fi
