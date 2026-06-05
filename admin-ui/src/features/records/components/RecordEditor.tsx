import React, { useState, useEffect } from 'react';
import { AlertCircle, Save } from 'lucide-react';
import { Button, Label, Badge } from '../../../components/ui/Elements';
import { TextInput } from '../../../components/form/TextInput';
import { Checkbox } from '../../../components/form/Checkbox';
import { Select } from '../../../components/form/Select';
import { RichTextEditor } from '../../../components/form/RichTextEditor';
import { JSONEditor } from '../../../components/form/JsonEditor';
import { FileUploader } from '../../../components/media/FileUploader';
import { RelationPicker } from '../../../components/form/RelationPicker';
import { UserPicker } from '../../../components/form/UserPicker';
import { Collection, SchemaField, AppRecord } from '../../../types';
import { FIELD_TYPES_CONFIG } from '../../../config/field-types.config';
import { validateRecord, ValidationError } from '../../../lib/schemaValidators';
import { filesService } from '../../files/services/filesService';
import { useToast } from '../../../components/feedback/Toast';
import {
  markdownHelper,
  turndownHelper,
} from '@/src/components/texteditor/components/GeminiEditor';

interface RecordEditorProps {
  collection: Collection;
  record?: AppRecord;
  onSave: (data: any) => Promise<void>;
  onCancel: () => void;
  depth?: number;
}

export const RecordEditor = ({
  collection,
  record,
  onSave,
  onCancel,
  depth = 0,
}: RecordEditorProps) => {
  const [formData, setFormData] = useState<any>({});
  const [errors, setErrors] = useState<ValidationError[]>([]);
  const [isSaving, setIsSaving] = useState(false);
  const { toast } = useToast();

  useEffect(() => {
    if (record) {
      collection.schema.forEach((f) => {
        if (f.type === 'text') {
          try {
            record[f.name] = markdownHelper(record[f.name]);
          } catch (error) {
            console.log(error);
          }
        }
      });
      setFormData({ ...record });
    } else {
      const defaults: any = {};
      collection.schema.forEach((f) => {
        if (f.type === 'bool') defaults[f.name] = f.default ?? false;
        else if (f.type === 'json') defaults[f.name] = f.default ?? '{}';
        else if (f.type === 'number') defaults[f.name] = f.default ?? null;
        else defaults[f.name] = f.default ?? '';
      });
      setFormData(defaults);
    }
  }, [record, collection]);

  const handleBlobUpload = (files: FileList, fieldName: string) => {
    const file = files[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onloadend = () => {
      const result = reader.result as string;
      const base64 = result.split(',')[1];
      setFormData((prev: any) => ({ ...prev, [fieldName]: base64 }));
    };
    reader.readAsDataURL(file);
  };

  const handleFileUpload = async (files: FileList, fieldName: string) => {
    const file = files[0];
    if (!file) return;

    toast('Uploading file...', 'info');

    try {
      const uploadedFile = await filesService.upload(file);
      setFormData((prev: any) => ({ ...prev, [fieldName]: uploadedFile.name }));
      setErrors((prev) => prev.filter((e) => e.field !== fieldName));
      toast('File uploaded successfully', 'success');
    } catch (e) {
      console.error(e);
      toast('Failed to upload file', 'error');
    }
  };

  const handleSave = async () => {
    // [FIX] Pre-process data: Parse JSON strings back to objects for submission
    const processedData = { ...formData };

    collection.schema.forEach((field) => {
      if (field.type === 'json' && typeof processedData[field.name] === 'string') {
        try {
          processedData[field.name] = JSON.parse(processedData[field.name]);
        } catch (e) {
          // If parse fails, leave as string (validation will likely catch it)
          console.warn(`Failed to parse JSON for field ${field.name}`, e);
        }
      }
      if (
        field.type === 'text' &&
        typeof processedData[field.name] === 'string' &&
        processedData[field.name].length > 0
      ) {
        try {
          processedData[field.name] = turndownHelper(processedData[field.name]);
        } catch (e) {
          // If turndown fails, leave as string (validation will likely catch it)
          console.warn(`Failed to turndown for field ${field.name}`, e);
        }
      }
    });

    // Validate against the PROCESSED data
    const validationErrors = validateRecord(processedData, collection.schema);
    if (validationErrors.length > 0) {
      setErrors(validationErrors);
      return;
    }

    setIsSaving(true);
    try {
      // Send the PROCESSED data (with actual arrays/objects)
      await onSave(processedData);
    } finally {
      setIsSaving(false);
    }
  };

  const renderInput = (field: SchemaField) => {
    const val = formData[field.name];
    const setter = (v: any) => {
      setFormData({ ...formData, [field.name]: v });
      if (errors.find((e) => e.field === field.name)) {
        setErrors((prev) => prev.filter((e) => e.field !== field.name));
      }
    };

    const fieldError = errors.find((e) => e.field === field.name)?.message;
    const config = FIELD_TYPES_CONFIG[field.type];
    const Icon = config?.icon;

    switch (field.type) {
      case 'bool':
        return <Checkbox label={field.name} checked={!!val} onChange={setter} error={fieldError} />;

      case 'select':
        return (
          <Select
            label={field.name}
            required={field.required}
            options={field.options || []}
            value={val || ''}
            onChange={(e) => setter(e.target.value)}
            icon={Icon && <Icon className="h-4 w-4" />}
            error={fieldError}
          />
        );

      case 'json':
        return (
          <div className="space-y-2">
            <div className="text-sm font-medium flex items-center gap-2">
              {Icon && <Icon className="h-4 w-4" />}
              {field.name}
              {field.required && <span className="text-destructive">*</span>}
            </div>
            <JSONEditor
              value={typeof val === 'string' ? val : JSON.stringify(val, null, 2)}
              onChange={setter}
              height="200px"
            />
            {fieldError && <span className="text-xs text-destructive">{fieldError}</span>}
          </div>
        );

      case 'text':
        return (
          <RichTextEditor
            label={field.name}
            value={val || ''}
            onChange={(e: any) => setter(e.target.value)}
            required={field.required}
            error={fieldError}
          />
        );

      case 'file':
        return (
          <div className="space-y-2">
            <div className="text-sm font-medium flex items-center gap-2">
              {Icon && <Icon className="h-4 w-4" />}
              {field.name}
              {field.required && <span className="text-destructive">*</span>}
            </div>
            <FileUploader onUpload={(files) => handleFileUpload(files, field.name)} />

            {val && (
              <div className="text-xs bg-secondary/20 p-2 rounded font-mono break-all flex justify-between items-center">
                <span>{val}</span>
                <span className="text-[10px] text-emerald-500 uppercase font-bold tracking-wider">
                  Linked
                </span>
              </div>
            )}

            {fieldError && <span className="text-xs text-destructive">{fieldError}</span>}
          </div>
        );

      case 'blob':
        return (
          <div className="space-y-2">
            <div className="text-sm font-medium flex items-center gap-2">
              {Icon && <Icon className="h-4 w-4" />}
              {field.name}{' '}
              <Badge variant="outline" className="text-[10px]">
                Base64
              </Badge>
            </div>
            <FileUploader onUpload={(files) => handleBlobUpload(files, field.name)} />
            {val && (
              <div className="text-xs bg-secondary/20 p-1 rounded font-mono truncate">
                Base64 Data ({val.length} chars)
              </div>
            )}
            {fieldError && <span className="text-xs text-destructive">{fieldError}</span>}
          </div>
        );

      case 'relation':
        return (
          <RelationPicker
            label={field.name}
            value={val || ''}
            onChange={setter}
            relationTo={field.relationTo || ''}
            depth={depth}
            error={fieldError}
          />
        );

      case 'owner':
        return (
          <UserPicker
            label={field.name}
            value={val || ''}
            onChange={setter}
            depth={depth}
            error={fieldError}
            required={field.required}
          />
        );

      case 'vector':
        return (
          <div className="space-y-2">
            <TextInput
              label={`${field.name} (Array or CSV)`}
              required={field.required}
              value={Array.isArray(val) ? JSON.stringify(val) : val || ''}
              onChange={(e) => {
                const txt = e.target.value;
                if (txt.startsWith('[') && txt.endsWith(']')) {
                  try {
                    setter(JSON.parse(txt));
                  } catch {
                    setter(txt);
                  }
                } else {
                  setter(
                    txt
                      .split(',')
                      .map((n) => parseFloat(n.trim()))
                      .filter((n) => !isNaN(n))
                  );
                }
              }}
              placeholder="[0.1, 0.5, ...]"
              icon={Icon && <Icon className="h-4 w-4" />}
              error={fieldError}
            />
            {field.dimension && (
              <p className="text-[10px] text-muted-foreground">
                Required dimension: {field.dimension}
              </p>
            )}
          </div>
        );

      case 'date':
        return (
          <TextInput
            type="datetime-local"
            label={field.name}
            required={field.required}
            value={val ? new Date(val).toISOString().slice(0, 16) : ''}
            onChange={(e) => setter(new Date(e.target.value).toISOString())}
            icon={Icon && <Icon className="h-4 w-4" />}
            error={fieldError}
          />
        );

      default:
        // string, number, email, url
        return (
          <TextInput
            label={field.name}
            required={field.required}
            value={val || ''}
            onChange={(e) =>
              setter(field.type === 'number' ? Number(e.target.value) : e.target.value)
            }
            type={field.type === 'number' ? 'number' : field.type === 'email' ? 'email' : 'text'}
            icon={Icon && <Icon className="h-4 w-4" />}
            error={fieldError}
          />
        );
    }
  };

  return (
    <>
      {errors.length > 0 && (
        <div className="m-4 mb-0 p-3 bg-destructive/10 border border-destructive/20 rounded-md flex items-start gap-2 text-destructive text-sm animate-in fade-in slide-in-from-top-1">
          <AlertCircle className="h-4 w-4 mt-0.5 shrink-0" />
          <div>
            <p className="font-semibold">Validation failed</p>
            <ul className="list-disc list-inside text-xs opacity-90 mt-1">
              {errors.map((e, i) => (
                <li key={i}>{e.message}</li>
              ))}
            </ul>
          </div>
        </div>
      )}

      <div className="flex-1 overflow-y-auto p-4 sm:p-6 space-y-6">
        {collection.schema.map((f) => (
          <div key={f.name}>{renderInput(f)}</div>
        ))}
      </div>

      <div className="p-4 border-t flex gap-3 bg-background safe-bottom">
        <Button variant="outline" onClick={onCancel} className="flex-1">
          Cancel
        </Button>
        <Button onClick={handleSave} isLoading={isSaving} className="flex-1">
          <Save className="mr-2 h-4 w-4" /> Save
        </Button>
      </div>
    </>
  );
};
