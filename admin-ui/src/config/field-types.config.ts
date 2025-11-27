
import { Type, Hash, CheckSquare, Mail, Link as LinkIcon, Calendar, MousePointerClick, FileCode, UploadCloud, Database } from 'lucide-react';

export const FIELD_TYPES_CONFIG = {
  text: { label: 'Text', icon: Type, description: 'Small to long strings' },
  number: { label: 'Number', icon: Hash, description: 'Integers or floats' },
  bool: { label: 'Boolean', icon: CheckSquare, description: 'True or false switch' },
  email: { label: 'Email', icon: Mail, description: 'Email address validation' },
  url: { label: 'URL', icon: LinkIcon, description: 'Link validation' },
  date: { label: 'Date', icon: Calendar, description: 'Date and time picker' },
  select: { label: 'Select', icon: MousePointerClick, description: 'Dropdown options' },
  json: { label: 'JSON', icon: FileCode, description: 'Raw JSON object' },
  file: { label: 'File', icon: UploadCloud, description: 'Single or multiple files' },
  relation: { label: 'Relation', icon: Database, description: 'Link to another record' },
};
