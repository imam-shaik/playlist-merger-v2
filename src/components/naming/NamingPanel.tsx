import React from 'react';
import { NamingModeSelector } from './NamingModeSelector';
import { NamingTemplateInput } from './NamingTemplateInput';
import { NamingAdvancedOptions } from './NamingAdvancedOptions';

export function NamingPanel() {
  return (
    <div className="space-y-4">
      <NamingModeSelector />
      <NamingTemplateInput />
      <NamingAdvancedOptions />
    </div>
  );
}