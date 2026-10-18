// Closed ordinary ECMAScript lowercase projection. No DOM/global writes.
function channelOrdinaryFields(request) {
  if (!request || typeof request.generation !== "string" || !request.generation ||
      !Number.isSafeInteger(request.stage) || request.stage < 0 ||
      !Array.isArray(request.fields) || request.fields.length > 128 ||
      request.fields.some(x => typeof x !== "string")) return null;
  return { generation: request.generation, stage: request.stage,
    fields: request.fields.slice(), values: request.fields.map(x => x.toLowerCase()) };
}
