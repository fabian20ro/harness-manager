import fs from 'node:fs';
import path from 'node:path';

function migrateClaudeToGemini() {
  const claudeDir = path.join('.claude', 'agents');
  const geminiDir = path.join('.gemini', 'plugins', 'skills');

  if (!fs.existsSync(claudeDir)) {
    console.log('No Claude agents found to migrate.');
    return;
  }

  if (!fs.existsSync(geminiDir)) {
    fs.mkdirSync(geminiDir, { recursive: true });
  }

  const files = fs.readdirSync(claudeDir);
  let migratedCount = 0;

  for (const file of files) {
    if (!file.endsWith('.md')) continue;

    const content = fs.readFileSync(path.join(claudeDir, file), 'utf8');
    
    // Naive mapping from Claude agent format to Gemini Skill format
    let newContent = content;
    
    // Add Gemini `<activated_skill>` wrapper if it doesn't have it
    if (!newContent.includes('<activated_skill>')) {
      const skillName = file.replace('.md', '');
      newContent = `<!-- Migrated from Claude Agent -->\n<activated_skill>\n${skillName}\n</activated_skill>\n\n<instructions>\n${newContent}\n</instructions>`;
    }
    
    fs.writeFileSync(path.join(geminiDir, file), newContent);
    migratedCount++;
    console.log(`Migrated ${file} -> .gemini/plugins/skills/${file}`);
  }

  console.log(`Migration complete! Migrated ${migratedCount} skills.`);
}

migrateClaudeToGemini();