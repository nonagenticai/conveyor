// S05 §7 property (c): the board card page under load, and the five-page walk.
//
// Run (the gate's command):
//   k6 run -e BASE_URL=$API -e BOARD_ID=$B -e DEV_AUTH_TOKEN=dev:alice tests/load/board.js
//
// Property (c) has two thresholds and the harness proves both:
//   * first page — `GET /api/boards/{id}/cards?limit=200` p95 ≤ 120 ms at 20 rps, no failed
//     request (the page the board screen actually reads);
//   * the walk — 1,000 cards in five cursor pages in ≤ 500 ms TOTAL. A cursor regression that
//     rescans or sorts per page leaves the first page fast while the walk blows the budget, so
//     the first-page threshold alone cannot see it. `five_page_ms` carries `max<=500`: every
//     walk, not just the median, must fit.
//
// `DEV_AUTH_TOKEN` is the CONTRACT §10 dev credential; a headerless request would be a 401, so
// the header is part of the test, not an afterthought.

import http from 'k6/http';
import { check } from 'k6';
import { Trend } from 'k6/metrics';

const fivePageMs = new Trend('five_page_ms', true);

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
    five_pages: {
      // Five independent walks, each walking the full board once. Enough samples for the
      // threshold to mean something without turning the proof into a second load test.
      executor: 'per-vu-iterations',
      exec: 'fivePages',
      vus: 5,
      iterations: 1,
      startTime: '2s',
    },
  },
  thresholds: {
    // The gate's number, in milliseconds.
    'http_req_duration{name:board_cards}': ['p(95)<=120'],
    // A 4xx/5xx response is a failed iteration even if it is fast.
    'http_req_failed{name:board_cards}': ['rate==0'],
    // The whole five-page walk, every iteration.
    five_page_ms: ['max<=500'],
    // Any failed assertion fails the gate: a page that answered non-200 or a walk that read the
    // wrong number of cards is a regression even when every request was fast.
    checks: ['rate==1'],
  },
};

const BASE_URL = __ENV.BASE_URL || 'http://127.0.0.1:8930';
const BOARD_ID = __ENV.BOARD_ID;
const DEV_AUTH_TOKEN = __ENV.DEV_AUTH_TOKEN;
const PAGE = 200;
const PAGES = 5;

const headers = {
  authorization: `Bearer ${DEV_AUTH_TOKEN}`,
  accept: 'application/json',
};

export default function () {
  const response = http.get(`${BASE_URL}/api/boards/${BOARD_ID}/cards?limit=${PAGE}`, {
    headers,
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

// Each VU of the `five_pages` scenario runs this once.
export function fivePages() {
  let cursor = null;
  let pages = 0;
  let cards = 0;
  let total = 0;
  let ok = true;
  while (pages < PAGES) {
    const query = cursor
      ? `?limit=${PAGE}&cursor=${encodeURIComponent(cursor)}`
      : `?limit=${PAGE}`;
    const response = http.get(`${BASE_URL}/api/boards/${BOARD_ID}/cards${query}`, {
      headers,
      tags: { name: 'board_walk' },
    });
    total += response.timings.duration;
    if (response.status !== 200) {
      ok = false;
      break;
    }
    const body = response.json();
    cards += Array.isArray(body.items) ? body.items.length : 0;
    cursor = body.next_cursor || null;
    pages += 1;
    if (!cursor) {
      break;
    }
  }
  fivePageMs.add(total);
  check(null, {
    'the walk served every page': () => ok,
    'the walk read five pages': () => pages === PAGES,
    'the walk read 1,000 cards': () => cards === 1000,
    'the walk used every cursor': () => cursor === null || pages === PAGES,
  });
}
