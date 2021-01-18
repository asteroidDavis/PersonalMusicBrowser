# Generated by Django 3.1.5 on 2021-01-18 02:10

from django.db import migrations, models


class Migration(migrations.Migration):

    dependencies = [
        ('Discography', '0001_initial'),
    ]

    operations = [
        migrations.AddField(
            model_name='recording',
            name='notes',
            field=models.ImageField(blank=True, upload_to=''),
        ),
        migrations.AlterField(
            model_name='cover',
            name='notes',
            field=models.ImageField(blank=True, upload_to=''),
        ),
        migrations.RemoveField(
            model_name='recording',
            name='instruments',
        ),
        migrations.AddField(
            model_name='recording',
            name='instruments',
            field=models.ManyToManyField(to='Discography.Instrument'),
        ),
        migrations.AlterField(
            model_name='recording',
            name='type',
            field=models.TextField(choices=[('audacity', 'audacity'), ('mix', 'mix'), ('master', 'master'), ('loop-core-list', 'loop-core-list'), ('wav', 'wav'), ('audacity', 'audacity')], max_length=64),
        ),
    ]
