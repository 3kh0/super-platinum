// Chime SDK messaging (chat channels) pulls in node:https and node:fs. Meetings
// never touch it, and a browser bundle cannot resolve those modules.
module.exports = {};
