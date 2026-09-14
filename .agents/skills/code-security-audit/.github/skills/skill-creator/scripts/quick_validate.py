#!/usr/bin/env python3
"""
Skill Validator - Validates skill structure and content

Usage:
    quick_validate.py <path/to/skill-folder>

Example:
    quick_validate.py skills/public/my-skill
"""

import sys
import re
from pathlib import Path


def validate_skill(skill_path):
    """
    Validate a skill folder structure and content.
    
    Args:
        skill_path: Path to the skill folder
        
    Returns:
        tuple: (is_valid, errors, warnings)
    """
    skill_path = Path(skill_path).resolve()
    errors = []
    warnings = []
    
    # Check skill folder exists
    if not skill_path.exists():
        errors.append(f"Skill folder does not exist: {skill_path}")
        return False, errors, warnings
    
    if not skill_path.is_dir():
        errors.append(f"Path is not a directory: {skill_path}")
        return False, errors, warnings
    
    # Check skill name format (folder name)
    skill_name = skill_path.name
    name_pattern = r'^[a-z][a-z0-9-]*[a-z0-9]$|^[a-z]$'
    if not re.match(name_pattern, skill_name):
        errors.append(f"Invalid skill folder name: '{skill_name}'. Must be hyphen-case (e.g., 'my-skill')")
    
    if '--' in skill_name:
        errors.append(f"Skill name contains double hyphens: '{skill_name}'")
    
    if len(skill_name) > 40:
        errors.append(f"Skill name too long: {len(skill_name)} chars (max 40)")
    
    # Check SKILL.md exists
    skill_md_path = skill_path / "SKILL.md"
    if not skill_md_path.exists():
        errors.append("Missing required file: SKILL.md")
        return False, errors, warnings
    
    # Read and validate SKILL.md
    try:
        content = skill_md_path.read_text()
    except Exception as e:
        errors.append(f"Cannot read SKILL.md: {e}")
        return False, errors, warnings
    
    # Check frontmatter exists and is valid
    if not content.startswith('---'):
        errors.append("SKILL.md must start with YAML frontmatter (---)")
    else:
        # Find closing frontmatter
        parts = content.split('---', 2)
        if len(parts) < 3:
            errors.append("SKILL.md frontmatter not properly closed with ---")
        else:
            frontmatter = parts[1].strip()
            
            # Check required fields
            if 'name:' not in frontmatter:
                errors.append("Missing required frontmatter field: name")
            else:
                # Extract and validate name matches folder
                name_match = re.search(r'^name:\s*(.+)$', frontmatter, re.MULTILINE)
                if name_match:
                    fm_name = name_match.group(1).strip()
                    if fm_name != skill_name:
                        errors.append(f"Frontmatter name '{fm_name}' doesn't match folder name '{skill_name}'")
            
            if 'description:' not in frontmatter:
                errors.append("Missing required frontmatter field: description")
            else:
                # Check description is not a TODO
                desc_match = re.search(r'^description:\s*(.+)$', frontmatter, re.MULTILINE | re.DOTALL)
                if desc_match:
                    description = desc_match.group(1).strip()
                    if '[TODO' in description or description == '':
                        errors.append("Description contains TODO placeholder or is empty")
                    elif len(description) < 50:
                        warnings.append(f"Description is short ({len(description)} chars). Consider adding more detail about when to use this skill.")
            
            # Check for unauthorized frontmatter fields
            allowed_fields = {'name', 'description', 'license'}
            field_pattern = r'^(\w+):'
            for line in frontmatter.split('\n'):
                match = re.match(field_pattern, line)
                if match:
                    field = match.group(1)
                    if field not in allowed_fields:
                        warnings.append(f"Unexpected frontmatter field: '{field}'. Only 'name' and 'description' are required.")
    
    # Check for TODO items in body
    if '[TODO' in content.split('---', 2)[-1] if len(content.split('---', 2)) > 2 else content:
        warnings.append("SKILL.md body contains [TODO] placeholders that should be completed")
    
    # Check for prohibited files
    prohibited_files = ['README.md', 'INSTALLATION_GUIDE.md', 'QUICK_REFERENCE.md', 'CHANGELOG.md']
    for filename in prohibited_files:
        if (skill_path / filename).exists():
            warnings.append(f"Found unnecessary file: {filename}. Skills should only contain essential files.")
    
    # Check resource directories
    for resource_dir in ['scripts', 'references', 'assets']:
        dir_path = skill_path / resource_dir
        if dir_path.exists():
            if not dir_path.is_dir():
                errors.append(f"'{resource_dir}' exists but is not a directory")
            else:
                files = list(dir_path.iterdir())
                if not files:
                    warnings.append(f"'{resource_dir}/' directory is empty. Delete if not needed.")
    
    is_valid = len(errors) == 0
    return is_valid, errors, warnings


def main():
    if len(sys.argv) < 2:
        print("Usage: quick_validate.py <path/to/skill-folder>")
        print("\nExample:")
        print("  quick_validate.py skills/public/my-skill")
        sys.exit(1)
    
    skill_path = sys.argv[1]
    
    print(f"🔍 Validating skill: {skill_path}")
    print()
    
    is_valid, errors, warnings = validate_skill(skill_path)
    
    if errors:
        print("❌ Errors:")
        for error in errors:
            print(f"   • {error}")
        print()
    
    if warnings:
        print("⚠️  Warnings:")
        for warning in warnings:
            print(f"   • {warning}")
        print()
    
    if is_valid:
        print("✅ Skill validation passed!")
        if warnings:
            print("   (with warnings - consider addressing them)")
        sys.exit(0)
    else:
        print("❌ Skill validation failed. Fix errors and try again.")
        sys.exit(1)


if __name__ == "__main__":
    main()
