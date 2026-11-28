// S4's retained V1 rows: the board index and the card cockpit. `/board/:board_id` stays with S1's
// plan module; these two are the routes the walk still lacked at S4's claim.

export default [
  {
    walk: "/boards",
    app: "/boards",
    room: "portfolio",
    spec: "e2e/v1/board.spec.ts",
    load: () => import("../../destinations/v1/boards.js"),
  },
  {
    walk: "/card/:card_id",
    app: "/card/:card_id",
    room: "portfolio",
    spec: "e2e/v1/cockpit.spec.ts",
    load: () => import("../../destinations/v1/cockpit.js"),
  },
];
