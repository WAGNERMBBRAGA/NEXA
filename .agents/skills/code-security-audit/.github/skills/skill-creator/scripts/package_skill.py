#!/usr/bin/env python3
"""
Skill Packager - Creates a distributable .skill file of a skill folder

Usage:
    package_skill.py <path/to/skill-folder> [output-directory]

Example:
    package_skill.py skills/public/my-skill
    package_skill.py skills/public/my-skill ./dist
"""

import sys
import zipfile
from pathlib import Path

from quick_validate import validate_skill


def package_skill(skill_path, output_dir=None):
    """
    Package a skill folder into a .skill file.
    
    Args:
        skill_path: Path to the skill folder
        output_dir: Optional output directory for the .skill file (defaults to current directory)
        
    Returns:
        Path to the created .skill file, or None if error
    """
    skill_path = Path(skill_path).resolve()
    
    # Validate skill folder exists
    if not skill_path.exists():
        print(f"❌ Error: Skill folder does not exist: {skill_path}")
        return None
    
    if not skill_path.is_dir():
        print(f"❌ Error: Path is not a directory: {skill_path}")
        return None
    
    skill_name = skill_path.name
    
    # Validate skill before packaging
    print("🔍 Validating skill...")
    is_valid, errors, warnings = validate_skill(skill_path)
    
    if errors:
        print("\n❌ Validation errors:")
        for error in errors:
            print(f"   • {error}")
        print("\n❌ Fix validation errors before packaging.")
        return None
    
    if warnings:
        print("\n⚠️  Validation warnings:")
        for warning in warnings:
            print(f"   • {warning}")
        print("\n   Proceeding with packaging despite warnings...")
    
    print("✅ Validation passed\n")
    
    # Determine output path
    if output_dir:
        output_path = Path(output_dir).resolve()
        output_path.mkdir(parents=True, exist_ok=True)
    else:
        output_path = Path.cwd()
    
    skill_filename = output_path / f"{skill_name}.skill"
    
    # Create the .skill file (zip format)
    try:
        print(f"📦 Creating package: {skill_filename}")
        
        with zipfile.ZipFile(skill_filename, 'w', zipfile.ZIP_DEFLATED) as zipf:
            # Walk through the skill directory
            for file_path in skill_path.rglob('*'):
                if file_path.is_file():
                    # Skip common unwanted files
                    if file_path.name.startswith('.'):
                        continue
                    if file_path.name.endswith('.pyc'):
                        continue
                    if '__pycache__' in str(file_path):
                        continue
                    
                    # Calculate the relative path within the zip
                    arcname = file_path.relative_to(skill_path.parent)
                    zipf.write(file_path, arcname)
                    print(f"   Added: {arcname}")
        
        print(f"\n✅ Successfully packaged skill to: {skill_filename}")
        return skill_filename
        
    except Exception as e:
        print(f"❌ Error creating .skill file: {e}")
        return None


def main():
    if len(sys.argv) < 2:
        print("Usage: package_skill.py <path/to/skill-folder> [output-directory]")
        print("\nExample:")
        print("  package_skill.py skills/public/my-skill")
        print("  package_skill.py skills/public/my-skill ./dist")
        sys.exit(1)
    
    skill_path = sys.argv[1]
    output_dir = sys.argv[2] if len(sys.argv) > 2 else None
    
    print(f"📦 Packaging skill: {skill_path}")
    if output_dir:
        print(f"   Output directory: {output_dir}")
    print()
    
    result = package_skill(skill_path, output_dir)
    
    if result:
        sys.exit(0)
    else:
        sys.exit(1)


if __name__ == "__main__":
    main()
