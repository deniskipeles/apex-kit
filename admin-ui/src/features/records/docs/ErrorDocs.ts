export const ErrorDocs = {
  responseFormat: `{
  "error": "not_found",           // Short code
  "message": "Record not found",  // Human readable message
  "status": 404,                  // HTTP Status Code
  "details": { ... }              // Optional validation arrays
}`,
};
