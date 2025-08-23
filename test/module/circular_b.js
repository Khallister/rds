const c = require("./circular_c");
const a = require("./circular_a");

module.exports = {
  name: "circular_b",
  deps: { c, a },
};
