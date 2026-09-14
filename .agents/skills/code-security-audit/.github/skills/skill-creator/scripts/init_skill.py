#!/usr/bin/env python3
"""
Skill Initializer - Creates a new skill from template

Usage:
    init_skill.py <skill-name> --path <output-directory>

Examples:
    init_skill.py my-new-skill --path skills/public
    init_skill.py my-api-helper --path skills/private
    init_skill.py custom-skill --path /custom/location
"""

import sys
import re
from pathlib import Path


SKILL_TEMPLATE = """---
name: {skill_name}
description: [TODO: Complete and informative explanation of what the skill does and when to use it. Include WHEN to use this skill - specific scenarios, file types, or tasks that trigger it.]
---

# {skill_title}

## Overview

[TODO: 1-2 sentences explaining what this skill enables]

## Structuring This Skill

[TODO: Choose the structure that best fits this skill's purpose. Common patterns:

1. **Workflow-Based** (best for sequential processes):
   - Step-by-step instructions
   - Ordered operations
   - Checkpoints and validation

2. **Reference-Based** (best for domain knowledge):
   - Key concepts and definitions
   - Common patterns and examples
   - Lookup tables or schemas

3. **Tool-Based** (best for specific file/API operations):
   - Core operations
   - Input/output formats
   - Error handling

Delete this section once you've chosen and implemented your structure.]

## [Section 1]

[TODO: Add your content]

## [Section 2]

[TODO: Add your content]

## Bundled Resources

### Scripts

- `scripts/example_script.py` - [TODO: Describe what this script does]

### References

- `references/example_reference.md` - [TODO: Describe what this reference contains]

### Assets

- `assets/example_asset.txt` - [TODO: Describe what this asset is for]

[TODO: Delete any resource sections that aren't needed for this skill]
"""

EXAMPLE_SCRIPT = '''#!/usr/bin/env python3
"""
Example script - Replace with actual functionality

This is a placeholder script demonstrating proper structure.
Delete or replace this file as needed.
"""

import sys


def main():
    """Main entry point."""
    print("Example script executed successfully!")
    print(f"Arguments: {sys.argv[1:]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
'''

EXAMPLE_REFERENCE = """# Example Reference

This is a placeholder reference file. Replace with actual documentation.

## Section 1

Reference content goes here.

## Section 2

More reference content.

---

Delete this file if not needed for your skill.
"""

EXAMPLE_ASSET = """Example asset file.

This is a placeholder asset. Replace with actual assets like:
- Templates
- Configuration files
- Sample data
- Images (as actual image files)

Delete this file if not needed for your skill.
"""


def validate_skill_name(name):
    """Validate skill name follows conventions."""
    # Must be hyphen-case, lowercase, letters/digits/hyphens only
    pattern = r'^[a-z][a-z0-9-]*[a-z0-9]$|^[a-z]$'
    if not re.match(pattern, name):
        return False
    if len(name) > 40:
        return False
    if '--' in name:
        return False
    return True


def init_skill(skill_name, output_path):
    """
    Initialize a new skill directory with template files.
    
    Args:
        skill_name: Name of the skill (hyphen-case)
        output_path: Directory where skill folder will be created
        
    Returns:
        Path to created skill directory, or None if error
    """
    # Validate skill name
    if not validate_skill_name(skill_name):
        print(f"❌ Invalid skill name: '{skill_name}'")
        print("   - Must be hyphen-case (e.g., 'data-analyzer')")
        print("   - Lowercase letters, digits, and hyphens only")
        print("   - Max 40 characters")
        print("   - No double hyphens")
        return None
    
    # Convert skill name to title
    skill_title = ' '.join(word.capitalize() for word in skill_name.split('-'))
    
    # Create skill directory
    output_path = Path(output_path)
    skill_dir = output_path / skill_name
    
    if skill_dir.exists():
        print(f"❌ Error: Directory already exists: {skill_dir}")
        return None
    
    try:
        skill_dir.mkdir(parents=True, exist_ok=False)
        print(f"✅ Created skill directory: {skill_dir}")
    except Exception as e:
        print(f"❌ Error creating directory: {e}")
        return None
    
    # Create SKILL.md
    skill_md_path = skill_dir / "SKILL.md"
    try:
        skill_md_content = SKILL_TEMPLATE.format(
            skill_name=skill_name,
            skill_title=skill_title
        )
        skill_md_path.write_text(skill_md_content)
        print("✅ Created SKILL.md with template")
    except Exception as e:
        print(f"❌ Error creating SKILL.md: {e}")
        return None
    
    # Create resource directories and example files
    try:
        # Scripts
        scripts_dir = skill_dir / "scripts"
        scripts_dir.mkdir()
        example_script = scripts_dir / "example_script.py"
        example_script.write_text(EXAMPLE_SCRIPT)
        example_script.chmod(0o755)
        print("✅ Created scripts/example_script.py")
        
        # References
        references_dir = skill_dir / "references"
        references_dir.mkdir()
        example_reference = references_dir / "example_reference.md"
        example_reference.write_text(EXAMPLE_REFERENCE)
        print("✅ Created references/example_reference.md")
        
        # Assets
        assets_dir = skill_dir / "assets"
        assets_dir.mkdir()
        example_asset = assets_dir / "example_asset.txt"
        example_asset.write_text(EXAMPLE_ASSET)
        print("✅ Created assets/example_asset.txt")
        
    except Exception as e:
        print(f"❌ Error creating resource directories: {e}")
        return None
    
    # Print next steps
    print(f"\n✅ Skill '{skill_name}' initialized successfully at {skill_dir}")
    print("\nNext steps:")
    print("1. Edit SKILL.md to complete the TODO items and update the description")
    print("2. Customize or delete the example files in scripts/, references/, and assets/")
    print("3. Run the validator when ready to check the skill structure")
    
    return skill_dir


def main():
    if len(sys.argv) < 4 or sys.argv[2] != '--path':
        print("Usage: init_skill.py <skill-name> --path <output-directory>")
        print("\nSkill name requirements:")
        print("  - Hyphen-case identifier (e.g., 'data-analyzer')")
        print("  - Lowercase letters, digits, and hyphens only")
        print("  - Max 40 characters")
        print("  - Must match directory name exactly")
        print("\nExamples:")
        print("  init_skill.py my-new-skill --path skills/public")
        print("  init_skill.py my-api-helper --path skills/private")
        print("  init_skill.py custom-skill --path /custom/location")
        sys.exit(1)
    
    skill_name = sys.argv[1]
    path = sys.argv[3]
    
    print(f"🚀 Initializing skill: {skill_name}")
    print(f"   Location: {path}")
    print()
    
    result = init_skill(skill_name, path)
    
    if result:
        sys.exit(0)
    else:
        sys.exit(1)


if __name__ == "__main__":
    main()
