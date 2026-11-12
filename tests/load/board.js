// S05 §7 property (c): the board card page under load.
//
// Run (the gate's command):
//   k6 run -e BASE_URL=$API -e BOARD_ID=$B -e DEV_AUTH_TOKEN=dev:alice tests/load/board.js
//
// 20 requests/second for 60 seconds against `GET /api/boards/{id}/cards?limit=200` — the page
// the board screen actually reads. The assertion is the gate's: p95 ≤ 120 ms, with no failed
// request. `DEV_AUTH_TOKEN` is the CONTRACT §10 dev credential; a headerless request would be
// a 401, so the header is part of the test, not an afterthought.

import http from 'k6/http';
import { check } from 'k6';

export const options = {
  scenarios: {
    board_cards: {
      executor: 'constant-arrival-rate',
      rate: 20,
      timeUnit: '1s',
      duration: '60s',
      preAllocatedVUs: 20,
      maxVUs: 50,
    },
  },
  thresholds: {
    // The gate's number, in milliseconds.
    http_req_duration: ['p(95)<=120'],
    // A 4xx/5xx response is a failed iteration even if it is fast.
    http_req_failed: ['rate==0'],
  },
};

const BASE_URL = __ENV.BASE_URL || 'http://127.0.0.1:8930';
const BOARD_ID = __ENV.BOARD_ID;
const DEV_AUTH_TOKEN = __ENV.DEV_AUTH_TOKEN;

export default function () {
  const response = http.get(`${BASE_URL}/api/boards/${BOARD_ID}/cards?limit=200`, {
    headers: {
      authorization: `Bearer ${DEV_AUTH_TOKEN}`,
      accept: 'application/json',
    },
    tags: { name: 'board_cards' },
  });

  check(response, {
    'status is 200': (r) => r.status === 200,
    'page carries items': (r) => {
      if (r.status !== 200) {
        return false;
      }
      try {
        const body = r.json();
        return Array.isArray(body.items) && body.items.length > 0;
      } catch (error) {
        return false;
      }
    },
  });
}
