const a = require("./circular_a");

module.exports = {
  name: "circular_e",
  dependency: a, // This creates the circular dependency
};
