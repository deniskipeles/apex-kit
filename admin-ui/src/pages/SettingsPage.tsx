import React from 'react';
import { Save } from 'lucide-react';
import {
  Button,
  Input,
  Label,
  Card,
  CardHeader,
  CardTitle,
  CardContent,
} from '../components/ui/Elements';

export const SettingsPage = () => {
  return (
    <div className="p-6 max-w-4xl">
      <h2 className="text-3xl font-bold tracking-tight mb-6">Settings</h2>

      <div className="grid gap-6">
        <Card>
          <CardHeader>
            <CardTitle>Application</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="grid gap-2">
              <Label>App Name</Label>
              <Input defaultValue="My Awesome App" />
            </div>
            <div className="grid gap-2">
              <Label>App URL</Label>
              <Input defaultValue="https://app.apexkit.io" />
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>SMTP Configuration</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex items-center gap-4 p-4 border border-border rounded bg-secondary/20 text-sm text-muted-foreground">
              <div className="h-2 w-2 bg-destructive rounded-full animate-pulse"></div>
              SMTP is currently disabled.
            </div>
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label>Host</Label>
                <Input placeholder="smtp.example.com" />
              </div>
              <div className="space-y-2">
                <Label>Port</Label>
                <Input placeholder="587" />
              </div>
            </div>
          </CardContent>
        </Card>

        <div className="flex justify-end gap-2">
          <Button variant="ghost">Reset</Button>
          <Button>
            <Save className="mr-2 h-4 w-4" /> Save Changes
          </Button>
        </div>
      </div>
    </div>
  );
};
