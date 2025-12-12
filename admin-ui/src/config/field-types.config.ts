import { 
  Type, AlignLeft, Hash, CheckSquare, Mail, Link as LinkIcon, 
  Calendar, MousePointerClick, FileCode, UploadCloud, Database, 
  Binary, Braces, UserCheck 
} from 'lucide-react';

export const FIELD_TYPES_CONFIG = {
  string: { label: 'String', icon: Type, description: 'Short text (Title, Name)', color: 'text-blue-500' },
  text: { label: 'Text', icon: AlignLeft, description: 'Long text content', color: 'text-blue-400' },
  number: { label: 'Number', icon: Hash, description: 'Integers or floats', color: 'text-orange-500' },
  bool: { label: 'Boolean', icon: CheckSquare, description: 'True/false toggle', color: 'text-green-500' },
  email: { label: 'Email', icon: Mail, description: 'Email address validation', color: 'text-purple-500' },
  url: { label: 'URL', icon: LinkIcon, description: 'Link validation', color: 'text-cyan-500' },
  date: { label: 'Date', icon: Calendar, description: 'Date & time picker', color: 'text-pink-500' },
  select: { label: 'Select', icon: MousePointerClick, description: 'Dropdown options', color: 'text-yellow-500' },
  json: { label: 'JSON', icon: FileCode, description: 'Structured JSON data', color: 'text-red-500' },
  file: { label: 'File', icon: UploadCloud, description: 'File uploads', color: 'text-gray-500' },
  blob: { label: 'Blob', icon: Binary, description: 'Binary data (Base64)', color: 'text-gray-400' },
  relation: { label: 'Relation', icon: Database, description: 'Link to another record', color: 'text-emerald-500' },
  // vector: { label: 'Vector', icon: Braces, description: 'AI Embeddings array', color: 'text-indigo-500' },
  owner: { label: 'Owner', icon: UserCheck, description: 'User ID Reference', color: 'text-teal-500' },
};