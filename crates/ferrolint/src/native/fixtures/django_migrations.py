from django.db import migrations
from django.db.migrations import RunPython


def forward(apps, schema_editor):
    migrations.RunPython(forward)


class Migration(migrations.Migration):
    operations = [
        migrations.RunPython(forward),
        migrations.RunPython(forward, reverse_code=forward),
        migrations.RunPython(forward, forward),
        migrations.RunPython(code=forward),
        migrations.RunPython(code=forward, reverse_code=None),
        RunPython(forward),
        migrations.RunSQL("select 1"),
        migrations.RunPython(*[forward]),
        migrations.RunPython(**{"code": forward}),
    ]

    def nested(self):
        def inner():
            return migrations.RunPython(forward)

        return [migrations.RunPython(forward), inner]


migrations.RunPython(forward)
