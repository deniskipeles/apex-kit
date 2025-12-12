
import { Collection, AppRecord, SystemLog, AdminUser, StoredFile } from './types';

export const APEX_USER = "apex-user"
export const APEX_TOKEN = "apex-token"
export const APEX_AUTH = "apex-auth"
export const APEX_THEME = "apex-theme"
export const APEX_FILES_THUMB_SIZE = "100x100"
export const APEX_RICH_TEXT_EDITOR_DEFAULT = `
    <h2 class="text-2xl font-bold mb-2">Welcome to the Gemini Editor!</h2>
    <p>This is a demo of a TinyMCE-like editor powered by <strong>Google Gemini</strong>.</p>
    <p>You can edit this text directly, or use the magic wand button in the bottom-right corner to enhance your content. Try selecting some text and asking the AI to "summarize this" or "correct spelling and grammar".</p>
    <ul>
      <li class="ml-4 list-disc">Manually edit and format text.</li>
      <li class="ml-4 list-disc">Use AI for spell checking, data inflation, and content proving.</li>
      <li class="ml-4 list-disc">Powered by Gemini Flash and Google Search for up-to-date info.</li>
    </ul>
    <p><br></p>
  `

export const MOCK_COLLECTIONS: Collection[] = [];

export const MOCK_RECORDS: AppRecord[] = [];

export const MOCK_ADMIN_USERS: AdminUser[] = [];

export const MOCK_LOGS: SystemLog[] = [];

export const CHART_DATA = [];

export const MOCK_FILES: StoredFile[] = [];
